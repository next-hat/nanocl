use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  net::IpAddr,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::Arc,
  time::{Duration, Instant},
};

use futures::lock::{Mutex, MutexGuard};
use ntex::{rt, web};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::stubs::dns::ResourceDnsRule;

/// Fully resolved configuration for one bind address. A separate dnsmasq
/// process is used for each address so records do not leak between networks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DnsmasqScope {
  pub(crate) listen_address: Option<IpAddr>,
  pub(crate) entries: BTreeMap<String, BTreeSet<IpAddr>>,
}

impl DnsmasqScope {
  pub(crate) fn new(listen_address: IpAddr) -> Self {
    Self {
      listen_address: Some(listen_address),
      entries: BTreeMap::new(),
    }
  }

  pub(crate) fn add_entry(&mut self, name: String, address: IpAddr) {
    self.entries.entry(name).or_default().insert(address);
  }
}

/// State kept while resource hooks are in flight. Nanocld invokes a resource
/// controller before committing the corresponding database mutation, so the
/// override map closes the window between controller reconciliation and that
/// commit.
#[derive(Default)]
pub(crate) struct DnsmasqUpdate {
  pub(crate) overrides: BTreeMap<String, PendingDnsRule>,
}

#[derive(Clone)]
pub(crate) struct PendingDnsRule {
  pub(crate) desired: Option<ResourceDnsRule>,
  pub(crate) expires_at: Instant,
}

#[derive(Clone, Debug)]
struct InstancePaths {
  dir: PathBuf,
  config: PathBuf,
  candidate: PathBuf,
  pid: PathBuf,
}

impl InstancePaths {
  fn new(root: &Path, id: &str) -> Self {
    let dir = root.join(id);
    Self {
      config: dir.join("dnsmasq.conf"),
      candidate: dir.join("dnsmasq.conf.next"),
      pid: dir.join("dnsmasq.pid"),
      dir,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstanceAction {
  Keep,
  Start,
  Create,
  Replace,
  Remove,
}

fn classify_instance(
  current: Option<&str>,
  desired: Option<&str>,
  running: bool,
) -> InstanceAction {
  match (current, desired) {
    (Some(current), Some(desired)) if current == desired => {
      if running {
        InstanceAction::Keep
      } else {
        InstanceAction::Start
      }
    }
    (Some(_), Some(_)) => InstanceAction::Replace,
    (None, Some(_)) => InstanceAction::Create,
    (Some(_), None) => InstanceAction::Remove,
    (None, None) => InstanceAction::Keep,
  }
}

#[derive(Clone, Debug)]
struct InstanceSnapshot {
  paths: InstancePaths,
  config: Option<String>,
  was_running: bool,
  dir_existed: bool,
}

#[derive(Clone, Debug)]
struct InstanceChange {
  action: InstanceAction,
  snapshot: InstanceSnapshot,
  desired_config: Option<String>,
}

/// Manager for isolated dnsmasq processes, one per resolved bind address.
#[derive(Clone)]
pub struct Dnsmasq {
  config_dir: String,
  dns: Vec<String>,
  update_lock: Arc<Mutex<DnsmasqUpdate>>,
}

impl Dnsmasq {
  pub(crate) fn new(config_path: &str) -> Self {
    Self {
      config_dir: config_path.to_owned(),
      dns: Vec::new(),
      update_lock: Arc::new(Mutex::new(DnsmasqUpdate::default())),
    }
  }

  /// Set upstream DNS servers used for names not present in local rules.
  pub(crate) fn with_dns(mut self, dns: Vec<String>) -> Self {
    self.dns = dns;
    self
  }

  fn instances_dir(&self) -> PathBuf {
    Path::new(&self.config_dir).join("instances")
  }

  fn scope_id(address: IpAddr) -> String {
    match address {
      IpAddr::V4(address) => {
        format!("v4-{}", address.to_string().replace('.', "-"))
      }
      IpAddr::V6(address) => format!("v6-{:032x}", u128::from(address)),
    }
  }

  fn paths_for(&self, address: IpAddr) -> InstancePaths {
    InstancePaths::new(&self.instances_dir(), &Self::scope_id(address))
  }

  fn command_error(context: &str, output: std::process::Output) -> IoError {
    let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
    message.push_str(String::from_utf8_lossy(&output.stderr).as_ref());
    IoError::other(context, &message)
  }

  fn is_running(pid_path: &Path) -> bool {
    let Ok(pid) = fs::read_to_string(pid_path) else {
      return false;
    };
    Command::new("kill")
      .arg("-0")
      .arg(pid.trim())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .is_ok_and(|status| status.success())
  }

  fn start_instance(paths: &InstancePaths) -> IoResult<()> {
    if Self::is_running(&paths.pid) {
      return Ok(());
    }
    let _ = fs::remove_file(&paths.pid);
    let output = Command::new("dnsmasq")
      .arg(format!("--conf-file={}", paths.config.display()))
      .arg("--log-facility=-")
      .output()
      .map_err(|err| err.map_err_context(|| "unable to start dnsmasq"))?;
    if output.status.success() {
      return Ok(());
    }
    Err(Self::command_error("dnsmasq start", output))
  }

  fn stop_instance(paths: &InstancePaths) -> IoResult<()> {
    if !Self::is_running(&paths.pid) {
      let _ = fs::remove_file(&paths.pid);
      return Ok(());
    }
    let pid = fs::read_to_string(&paths.pid).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to read pid file {}", paths.pid.display())
      })
    })?;
    let output = Command::new("kill")
      .arg("-TERM")
      .arg(pid.trim())
      .output()
      .map_err(|err| err.map_err_context(|| "unable to stop dnsmasq"))?;
    if !output.status.success() && Self::is_running(&paths.pid) {
      return Err(Self::command_error("dnsmasq stop", output));
    }
    for _ in 0..20 {
      if !Self::is_running(&paths.pid) {
        let _ = fs::remove_file(&paths.pid);
        return Ok(());
      }
      std::thread::sleep(Duration::from_millis(50));
    }
    let output = Command::new("kill")
      .arg("-KILL")
      .arg(pid.trim())
      .output()
      .map_err(|err| err.map_err_context(|| "unable to kill dnsmasq"))?;
    if !output.status.success() && Self::is_running(&paths.pid) {
      return Err(Self::command_error("dnsmasq kill", output));
    }
    let _ = fs::remove_file(&paths.pid);
    Ok(())
  }

  fn validate_config(path: &Path) -> IoResult<()> {
    let output = Command::new("dnsmasq")
      .arg("--test")
      .arg(format!("--conf-file={}", path.display()))
      .output()
      .map_err(|err| {
        err.map_err_context(|| "unable to validate dnsmasq configuration")
      })?;
    if output.status.success() {
      return Ok(());
    }
    Err(Self::command_error("dnsmasq config", output))
  }

  fn render_config(
    &self,
    scope: &DnsmasqScope,
    pid_path: &Path,
  ) -> IoResult<String> {
    let listen_address = scope.listen_address.ok_or_else(|| {
      IoError::invalid_data("dnsmasq", "scope is missing a listen address")
    })?;
    let mut config = format!(
      "bind-dynamic\nno-resolv\nno-poll\nno-hosts\nproxy-dnssec\nlisten-address={listen_address}\npid-file={}\n",
      pid_path.display()
    );
    for dns in &self.dns {
      let dns = dns.parse::<IpAddr>().map_err(|err| {
        IoError::invalid_data(
          "dnsmasq upstream",
          &format!("invalid DNS server {dns}: {err}"),
        )
      })?;
      config.push_str(&format!("server={dns}\n"));
    }
    for (name, addresses) in &scope.entries {
      if name.is_empty()
        || name
          .chars()
          .any(|character| matches!(character, '\n' | '\r' | '/' | '='))
      {
        return Err(IoError::invalid_data(
          "dnsmasq entry",
          &format!("invalid DNS name {name:?}"),
        ));
      }
      for address in addresses {
        config.push_str(&format!("address=/{name}/{address}\n"));
      }
    }
    Ok(config)
  }

  fn configured_instances(&self) -> IoResult<Vec<InstancePaths>> {
    let root = self.instances_dir();
    let mut instances = Vec::new();
    for entry in fs::read_dir(&root).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to read dnsmasq instances in {}", root.display())
      })
    })? {
      let entry = entry?;
      if !entry.file_type()?.is_dir() {
        continue;
      }
      let id = entry.file_name();
      let id = id.to_string_lossy();
      let paths = InstancePaths::new(&root, &id);
      if paths.config.is_file() {
        instances.push(paths);
      }
    }
    instances.sort_by(|left, right| left.dir.cmp(&right.dir));
    Ok(instances)
  }

  fn instance_id(paths: &InstancePaths) -> IoResult<String> {
    paths
      .dir
      .file_name()
      .and_then(|name| name.to_str())
      .map(str::to_owned)
      .ok_or_else(|| {
        IoError::invalid_data(
          "dnsmasq instance",
          &format!("invalid instance path {}", paths.dir.display()),
        )
      })
  }

  fn discard_staged(changes: &BTreeMap<String, InstanceChange>) {
    for change in changes.values() {
      let _ = fs::remove_file(&change.snapshot.paths.candidate);
      if change.action == InstanceAction::Create && !change.snapshot.dir_existed
      {
        let _ = fs::remove_dir_all(&change.snapshot.paths.dir);
      }
    }
  }

  fn rollback_changes(
    changes: &BTreeMap<String, InstanceChange>,
  ) -> IoResult<()> {
    let mut errors = Vec::new();

    // Stop every process touched by the failed transaction before restoring
    // its previous configuration and running state.
    for change in changes.values() {
      if let Err(err) = Self::stop_instance(&change.snapshot.paths) {
        errors.push(err.to_string());
      }
    }

    for change in changes.values() {
      let snapshot = &change.snapshot;
      let _ = fs::remove_file(&snapshot.paths.candidate);
      let _ = fs::remove_file(&snapshot.paths.config);
      match &snapshot.config {
        Some(config) => {
          if let Err(err) = fs::create_dir_all(&snapshot.paths.dir) {
            errors.push(err.to_string());
            continue;
          }
          if let Err(err) = fs::write(&snapshot.paths.config, config) {
            errors.push(err.to_string());
          }
        }
        None if !snapshot.dir_existed => {
          if let Err(err) = fs::remove_dir_all(&snapshot.paths.dir)
            && err.kind() != std::io::ErrorKind::NotFound
          {
            errors.push(err.to_string());
          }
        }
        None => {}
      }
    }

    for change in changes.values() {
      let snapshot = &change.snapshot;
      if snapshot.was_running {
        if snapshot.config.is_none() {
          errors.push(format!(
            "cannot restore running instance without config {}",
            snapshot.paths.dir.display()
          ));
        } else if let Err(err) = Self::start_instance(&snapshot.paths) {
          errors.push(err.to_string());
        }
      }
    }

    if errors.is_empty() {
      Ok(())
    } else {
      Err(IoError::other(
        "dnsmasq rollback".to_owned(),
        errors.join("; "),
      ))
    }
  }

  fn rollback_error(
    original: IoError,
    changes: &BTreeMap<String, InstanceChange>,
  ) -> IoError {
    match Self::rollback_changes(changes) {
      Ok(()) => original,
      Err(rollback) => IoError::other(
        "dnsmasq reconcile".to_owned(),
        format!("{original}; rollback failed: {rollback}"),
      ),
    }
  }

  fn reconcile_blocking(&self, scopes: Vec<DnsmasqScope>) -> IoResult<()> {
    let mut desired = BTreeMap::<String, (InstancePaths, String)>::new();
    for scope in scopes {
      let address = scope.listen_address.ok_or_else(|| {
        IoError::invalid_data("dnsmasq", "scope is missing a listen address")
      })?;
      let id = Self::scope_id(address);
      let paths = self.paths_for(address);
      let config = self.render_config(&scope, &paths.pid)?;
      desired.insert(id, (paths, config));
    }

    let mut current = BTreeMap::<String, (InstancePaths, String, bool)>::new();
    for paths in self.configured_instances()? {
      let id = Self::instance_id(&paths)?;
      let config = fs::read_to_string(&paths.config).map_err(|err| {
        err.map_err_context(|| {
          format!("unable to read dnsmasq config {}", paths.config.display())
        })
      })?;
      let running = Self::is_running(&paths.pid);
      current.insert(id, (paths, config, running));
    }

    let ids = current
      .keys()
      .chain(desired.keys())
      .cloned()
      .collect::<BTreeSet<_>>();
    let mut changes = BTreeMap::<String, InstanceChange>::new();
    for id in ids {
      let current_instance = current.get(&id);
      let desired_instance = desired.get(&id);
      let running = current_instance
        .map(|(_, _, running)| *running)
        .unwrap_or(false);
      let action = classify_instance(
        current_instance.map(|(_, config, _)| config.as_str()),
        desired_instance.map(|(_, config)| config.as_str()),
        running,
      );
      if action == InstanceAction::Keep {
        continue;
      }
      let paths = desired_instance
        .map(|(paths, _)| paths.clone())
        .or_else(|| current_instance.map(|(paths, _, _)| paths.clone()))
        .expect("instance change must have current or desired paths");
      changes.insert(
        id,
        InstanceChange {
          action,
          snapshot: InstanceSnapshot {
            dir_existed: paths.dir.is_dir(),
            paths,
            config: current_instance.map(|(_, config, _)| config.clone()),
            was_running: running,
          },
          desired_config: desired_instance.map(|(_, config)| config.clone()),
        },
      );
    }

    // Validate every changed/new candidate before touching any running
    // instance. A staging failure only needs to discard temporary files.
    let stage_result = (|| -> IoResult<()> {
      for change in changes.values() {
        if !matches!(
          change.action,
          InstanceAction::Create | InstanceAction::Replace
        ) {
          continue;
        }
        let paths = &change.snapshot.paths;
        fs::create_dir_all(&paths.dir).map_err(|err| {
          err.map_err_context(|| {
            format!("unable to create dnsmasq instance {}", paths.dir.display())
          })
        })?;
        let config = change
          .desired_config
          .as_ref()
          .expect("changed instance must have desired config");
        fs::write(&paths.candidate, config).map_err(|err| {
          err.map_err_context(|| {
            format!(
              "unable to stage dnsmasq config {}",
              paths.candidate.display()
            )
          })
        })?;
        Self::validate_config(&paths.candidate)?;
      }
      Ok(())
    })();
    if let Err(err) = stage_result {
      Self::discard_staged(&changes);
      return Err(err);
    }

    let apply_result = (|| -> IoResult<()> {
      // Stop every old process that will be replaced or removed before any
      // configuration is committed.
      for change in changes.values() {
        if change.snapshot.was_running
          && matches!(
            change.action,
            InstanceAction::Create
              | InstanceAction::Replace
              | InstanceAction::Remove
          )
        {
          Self::stop_instance(&change.snapshot.paths)?;
        }
      }

      for change in changes.values() {
        if matches!(
          change.action,
          InstanceAction::Create | InstanceAction::Replace
        ) {
          fs::rename(
            &change.snapshot.paths.candidate,
            &change.snapshot.paths.config,
          )
          .map_err(|err| {
            err.map_err_context(|| {
              format!(
                "unable to commit dnsmasq config {}",
                change.snapshot.paths.config.display()
              )
            })
          })?;
        }
      }

      for change in changes.values() {
        if matches!(
          change.action,
          InstanceAction::Start
            | InstanceAction::Create
            | InstanceAction::Replace
        ) {
          Self::start_instance(&change.snapshot.paths)?;
        }
      }

      // Quarantine removed configs only after every desired process is live.
      // If any rename fails, rollback restores all desired and removed scopes.
      for change in changes.values() {
        if change.action == InstanceAction::Remove {
          let _ = fs::remove_file(&change.snapshot.paths.candidate);
          fs::rename(
            &change.snapshot.paths.config,
            &change.snapshot.paths.candidate,
          )
          .map_err(|err| {
            err.map_err_context(|| {
              format!(
                "unable to quarantine dnsmasq instance {}",
                change.snapshot.paths.dir.display()
              )
            })
          })?;
        }
      }
      Ok(())
    })();

    if let Err(err) = apply_result {
      return Err(Self::rollback_error(err, &changes));
    }

    // Removed configs are already outside the configured instance set. Cleanup
    // is best-effort so a harmless leftover directory cannot roll back a fully
    // committed, running configuration.
    for change in changes.values() {
      if change.action == InstanceAction::Remove {
        if let Err(err) = fs::remove_dir_all(&change.snapshot.paths.dir) {
          log::warn!(
            "dnsmasq::reconcile: unable to clean removed instance {}: {err}",
            change.snapshot.paths.dir.display()
          );
        }
      } else {
        let _ = fs::remove_file(&change.snapshot.paths.candidate);
      }
    }
    Ok(())
  }

  /// Serialize rule snapshot construction and filesystem reconciliation.
  pub(crate) async fn lock_updates(&self) -> MutexGuard<'_, DnsmasqUpdate> {
    self.update_lock.lock().await
  }

  /// Atomically validate and apply a complete desired DNS configuration while
  /// the caller holds [`Dnsmasq::lock_updates`]. Changed instances receive a
  /// hard restart.
  pub(crate) async fn apply_scopes(
    &self,
    scopes: Vec<DnsmasqScope>,
  ) -> IoResult<()> {
    let manager = self.clone();
    web::block(move || manager.reconcile_blocking(scopes)).await?;
    Ok(())
  }

  /// Create the manager directory and stop the pre-0.18 singleton instance,
  /// which otherwise keeps its old nanoclbr0 bind alive across upgrades.
  pub(crate) fn ensure(&self) -> IoResult<Self> {
    let root = self.instances_dir();
    fs::create_dir_all(&root).map_err(|err| {
      err.map_err_context(|| {
        format!(
          "unable to create dnsmasq instances directory {}",
          root.display()
        )
      })
    })?;
    let legacy = InstancePaths {
      dir: Path::new(&self.config_dir).to_path_buf(),
      config: Path::new(&self.config_dir).join("dnsmasq.conf"),
      candidate: Path::new(&self.config_dir).join("dnsmasq.conf.next"),
      pid: Path::new(&self.config_dir).join("dnsmasq.pid"),
    };
    if legacy.pid.is_file() {
      Self::stop_instance(&legacy)?;
    }
    Ok(self.clone())
  }

  /// Start all persisted instances. Normally reconciliation creates them;
  /// this method is also used by the process monitor after a crash.
  pub(crate) async fn start(&self) -> IoResult<()> {
    let _guard = self.update_lock.lock().await;
    let manager = self.clone();
    web::block(move || {
      for paths in manager.configured_instances()? {
        Self::start_instance(&paths)?;
      }
      Ok::<_, IoError>(())
    })
    .await?;
    Ok(())
  }

  pub(crate) fn spawn(&self) {
    let dnsmasq = self.clone();
    rt::Arbiter::new().handle().spawn(async move {
      loop {
        if let Err(err) = dnsmasq.start().await {
          log::warn!("dnsmasq::monitor: {err}");
        }
        ntex::time::sleep(Duration::from_secs(2)).await;
      }
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scope_ids_are_safe_and_deterministic() {
    assert_eq!(
      Dnsmasq::scope_id("172.30.0.1".parse().unwrap()),
      "v4-172-30-0-1"
    );
    assert_eq!(
      Dnsmasq::scope_id("::1".parse().unwrap()),
      "v6-00000000000000000000000000000001"
    );
  }

  #[test]
  fn rendered_scope_has_no_nanoclbr0_singleton_and_is_isolated() {
    let manager =
      Dnsmasq::new("/tmp/ncdns-test").with_dns(vec!["1.1.1.1".to_owned()]);
    let mut scope = DnsmasqScope::new("172.30.0.1".parse().unwrap());
    scope.add_entry("api.internal".to_owned(), "172.30.0.2".parse().unwrap());
    let config = manager
      .render_config(&scope, Path::new("/tmp/ncdns-test.pid"))
      .unwrap();
    assert!(config.contains("listen-address=172.30.0.1"));
    assert!(config.contains("address=/api.internal/172.30.0.2"));
    assert!(!config.contains("interface=nanoclbr0"));
  }

  #[test]
  fn invalid_names_cannot_inject_dnsmasq_directives() {
    let manager = Dnsmasq::new("/tmp/ncdns-test");
    let mut scope = DnsmasqScope::new("127.0.0.1".parse().unwrap());
    scope.add_entry("ok\nserver=evil".to_owned(), "127.0.0.1".parse().unwrap());
    assert!(
      manager
        .render_config(&scope, Path::new("/tmp/ncdns-test.pid"))
        .is_err()
    );
  }

  #[test]
  fn instance_actions_cover_transaction_phases() {
    assert_eq!(
      classify_instance(Some("same"), Some("same"), true),
      InstanceAction::Keep
    );
    assert_eq!(
      classify_instance(Some("same"), Some("same"), false),
      InstanceAction::Start
    );
    assert_eq!(
      classify_instance(None, Some("new"), false),
      InstanceAction::Create
    );
    assert_eq!(
      classify_instance(Some("old"), Some("new"), true),
      InstanceAction::Replace
    );
    assert_eq!(
      classify_instance(Some("old"), None, true),
      InstanceAction::Remove
    );
  }

  #[test]
  fn rollback_restores_old_configs_and_removes_new_instances() {
    let root = std::env::temp_dir()
      .join(format!("ncdns-rollback-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let replace_paths = InstancePaths::new(&root, "replace");
    fs::create_dir_all(&replace_paths.dir).unwrap();
    fs::write(&replace_paths.config, "new").unwrap();

    let remove_paths = InstancePaths::new(&root, "remove");
    fs::create_dir_all(&remove_paths.dir).unwrap();
    fs::write(&remove_paths.candidate, "old").unwrap();

    let create_paths = InstancePaths::new(&root, "create");
    fs::create_dir_all(&create_paths.dir).unwrap();
    fs::write(&create_paths.config, "new").unwrap();

    let changes = BTreeMap::from([
      (
        "replace".to_owned(),
        InstanceChange {
          action: InstanceAction::Replace,
          snapshot: InstanceSnapshot {
            paths: replace_paths.clone(),
            config: Some("old".to_owned()),
            was_running: false,
            dir_existed: true,
          },
          desired_config: Some("new".to_owned()),
        },
      ),
      (
        "remove".to_owned(),
        InstanceChange {
          action: InstanceAction::Remove,
          snapshot: InstanceSnapshot {
            paths: remove_paths.clone(),
            config: Some("old".to_owned()),
            was_running: false,
            dir_existed: true,
          },
          desired_config: None,
        },
      ),
      (
        "create".to_owned(),
        InstanceChange {
          action: InstanceAction::Create,
          snapshot: InstanceSnapshot {
            paths: create_paths.clone(),
            config: None,
            was_running: false,
            dir_existed: false,
          },
          desired_config: Some("new".to_owned()),
        },
      ),
    ]);

    Dnsmasq::rollback_changes(&changes).unwrap();

    assert_eq!(fs::read_to_string(replace_paths.config).unwrap(), "old");
    assert_eq!(fs::read_to_string(remove_paths.config).unwrap(), "old");
    assert!(!create_paths.dir.exists());
    fs::remove_dir_all(root).unwrap();
  }
}
