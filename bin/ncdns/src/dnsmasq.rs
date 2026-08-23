use std::{
  collections::{BTreeMap, BTreeSet},
  ffi::OsStr,
  fs,
  net::IpAddr,
  path::{Path, PathBuf},
  process::{Command, Stdio},
  sync::{Arc, mpsc},
  time::Duration,
};

use futures::lock::Mutex;
use ntex::{rt, web};

use nanocl_error::io::{FromIo, IoError, IoResult};

/// Fully resolved configuration for one bind address. A separate dnsmasq
/// process is used for each address so records do not leak between networks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DnsmasqScope {
  pub(crate) listen_address: IpAddr,
  pub(crate) entries: BTreeMap<String, BTreeSet<IpAddr>>,
}

impl DnsmasqScope {
  pub(crate) fn new(listen_address: IpAddr) -> Self {
    Self {
      listen_address,
      entries: BTreeMap::new(),
    }
  }

  pub(crate) fn add_entry(&mut self, name: String, address: IpAddr) {
    self.entries.entry(name).or_default().insert(address);
  }
}

#[derive(Clone, Debug)]
struct InstancePaths {
  dir: PathBuf,
  config: PathBuf,
  candidate: PathBuf,
  pid: PathBuf,
  rules: PathBuf,
}

impl InstancePaths {
  fn new(root: &Path, id: &str) -> Self {
    let dir = root.join(id);
    Self {
      config: dir.join("dnsmasq.conf"),
      candidate: dir.join("dnsmasq.conf.next"),
      pid: dir.join("dnsmasq.pid"),
      rules: dir.join("rules"),
      dir,
    }
  }

  fn rule(&self, filename: &str) -> PathBuf {
    self.rules.join(filename)
  }

  fn rule_candidate(&self, filename: &str) -> PathBuf {
    self.rules.join(format!("{filename}.next"))
  }
}

#[derive(Clone, Debug)]
struct InstanceSnapshot {
  paths: InstancePaths,
  config: Option<String>,
  rule: Option<String>,
  was_running: bool,
  dir_existed: bool,
}

/// Manager for isolated dnsmasq processes, one per resolved bind address.
#[derive(Clone)]
pub struct Dnsmasq {
  config_dir: String,
  dns: Vec<String>,
  update_lock: Arc<Mutex<()>>,
}

impl Dnsmasq {
  pub(crate) fn new(config_path: &str) -> Self {
    Self {
      config_dir: config_path.to_owned(),
      dns: Vec::new(),
      update_lock: Arc::new(Mutex::new(())),
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
    // Keep dnsmasq as our direct child. Besides avoiding inherited capture
    // pipes, this prevents its double-forked intermediate process from
    // becoming a zombie when ncdns is PID 1 in the production container.
    let mut child = Command::new("dnsmasq")
      .arg("--keep-in-foreground")
      .arg(format!("--conf-file={}", paths.config.display()))
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map_err(|err| err.map_err_context(|| "unable to start dnsmasq"))?;
    let pid = child.id();
    let (exit_tx, exit_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
      let _ = exit_tx.send(child.wait());
    });

    // dnsmasq writes its pid file only after initialization succeeds. Wait for
    // that signal so callers never commit an instance that failed to start.
    for _ in 0..20 {
      match exit_rx.try_recv() {
        Ok(Ok(status)) => {
          return Err(IoError::other(
            "dnsmasq start",
            &format!("dnsmasq failed to start with status {status}"),
          ));
        }
        Ok(Err(err)) => {
          return Err(IoError::other(
            "dnsmasq start",
            &format!("unable to wait for dnsmasq: {err}"),
          ));
        }
        Err(mpsc::TryRecvError::Disconnected) => {
          return Err(IoError::other(
            "dnsmasq start",
            "dnsmasq waiter disconnected",
          ));
        }
        Err(mpsc::TryRecvError::Empty) => {}
      }
      if Self::is_running(&paths.pid) {
        return Ok(());
      }
      std::thread::sleep(Duration::from_millis(50));
    }

    let _ = Command::new("kill")
      .arg("-TERM")
      .arg(pid.to_string())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status();
    Err(IoError::other(
      "dnsmasq start",
      "dnsmasq did not create its pid file",
    ))
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

  fn render_instance_config(
    &self,
    listen_address: IpAddr,
    paths: &InstancePaths,
  ) -> IoResult<String> {
    let mut config = format!(
      "bind-dynamic\nno-resolv\nno-poll\nno-hosts\nproxy-dnssec\nlisten-address={listen_address}\npid-file={}\nconf-dir={},*.conf\n",
      paths.pid.display(),
      paths.rules.display()
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
    Ok(config)
  }

  fn render_rule_config(scope: &DnsmasqScope) -> IoResult<String> {
    let mut config = String::new();
    for (name, addresses) in &scope.entries {
      if name.is_empty()
        || name.chars().any(|character| {
          character.is_whitespace()
            || matches!(character, '\n' | '\r' | '/' | '=')
        })
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

  fn rule_filename(name: &str) -> IoResult<String> {
    if name.is_empty()
      || name.starts_with('.')
      || name.contains(['/', '\\', '\0'])
    {
      return Err(IoError::invalid_data(
        "dnsmasq rule",
        &format!("Resource name {name:?} is not a safe filename"),
      ));
    }
    Ok(format!("{name}.conf"))
  }

  fn read_optional(path: &Path) -> IoResult<Option<String>> {
    match fs::read_to_string(path) {
      Ok(content) => Ok(Some(content)),
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
      Err(err) => Err(*err.map_err_context(|| {
        format!("unable to read dnsmasq file {}", path.display())
      })),
    }
  }

  fn rule_fragments(paths: &InstancePaths) -> IoResult<Vec<PathBuf>> {
    let entries = match fs::read_dir(&paths.rules) {
      Ok(entries) => entries,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
        return Ok(Vec::new());
      }
      Err(err) => {
        return Err(*err.map_err_context(|| {
          format!(
            "unable to read dnsmasq rules directory {}",
            paths.rules.display()
          )
        }));
      }
    };
    let mut rules = Vec::new();
    for entry in entries {
      let entry = entry?;
      if entry.file_type()?.is_file()
        && entry.path().extension() == Some(OsStr::new("conf"))
      {
        rules.push(entry.path());
      }
    }
    rules.sort();
    Ok(rules)
  }

  fn has_rule_fragments(paths: &InstancePaths) -> IoResult<bool> {
    Ok(!Self::rule_fragments(paths)?.is_empty())
  }

  fn find_rule_instance(
    &self,
    filename: &str,
  ) -> IoResult<Option<InstancePaths>> {
    let root = self.instances_dir();
    let entries = match fs::read_dir(&root) {
      Ok(entries) => entries,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
        return Ok(None);
      }
      Err(err) => {
        return Err(*err.map_err_context(|| {
          format!("unable to read dnsmasq instances in {}", root.display())
        }));
      }
    };
    let mut found = None;
    for entry in entries {
      let entry = entry?;
      if !entry.file_type()?.is_dir() {
        continue;
      }
      let id = entry.file_name();
      let id = id.to_string_lossy();
      let paths = InstancePaths::new(&root, &id);
      if !paths.rule(filename).is_file() {
        continue;
      }
      if found.is_some() {
        return Err(IoError::invalid_data(
          "dnsmasq rule",
          &format!("Resource fragment {filename} exists in multiple instances"),
        ));
      }
      found = Some(paths);
    }
    Ok(found)
  }

  fn stage_rule(
    paths: &InstancePaths,
    filename: &str,
    config: &str,
  ) -> IoResult<()> {
    fs::create_dir_all(&paths.rules).map_err(|err| {
      err.map_err_context(|| {
        format!(
          "unable to create dnsmasq rules directory {}",
          paths.rules.display()
        )
      })
    })?;
    let candidate = paths.rule_candidate(filename);
    fs::write(&candidate, config).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to stage dnsmasq rule {}", candidate.display())
      })
    })?;
    Ok(())
  }

  fn commit_rule(paths: &InstancePaths, filename: &str) -> IoResult<()> {
    let candidate = paths.rule_candidate(filename);
    let rule = paths.rule(filename);
    fs::rename(&candidate, &rule).map_err(|err| {
      err.map_err_context(|| {
        format!("unable to commit dnsmasq rule {}", rule.display())
      })
    })?;
    Ok(())
  }

  /// Quarantine one Resource fragment. If it was the final fragment, also
  /// quarantine the root config so the process monitor cannot restart an empty
  /// instance while best-effort directory cleanup is pending.
  fn remove_rule_file(paths: &InstancePaths, filename: &str) -> IoResult<bool> {
    let candidate = paths.rule_candidate(filename);
    let _ = fs::remove_file(&candidate);
    fs::rename(paths.rule(filename), &candidate).map_err(|err| {
      err.map_err_context(|| {
        format!(
          "unable to remove dnsmasq rule {}",
          paths.rule(filename).display()
        )
      })
    })?;
    let has_rules = Self::has_rule_fragments(paths)?;
    if has_rules {
      return Ok(true);
    }
    let _ = fs::remove_file(&paths.candidate);
    match fs::rename(&paths.config, &paths.candidate) {
      Ok(()) => {}
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
      Err(err) => {
        return Err(*err.map_err_context(|| {
          format!("unable to retire dnsmasq instance {}", paths.dir.display())
        }));
      }
    }
    Ok(false)
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

  fn snapshot(
    paths: &InstancePaths,
    filename: &str,
  ) -> IoResult<InstanceSnapshot> {
    Ok(InstanceSnapshot {
      config: Self::read_optional(&paths.config)?,
      rule: Self::read_optional(&paths.rule(filename))?,
      was_running: Self::is_running(&paths.pid),
      dir_existed: paths.dir.is_dir(),
      paths: paths.clone(),
    })
  }

  fn discard_staged(snapshot: &InstanceSnapshot, filename: &str) {
    let _ = fs::remove_file(&snapshot.paths.candidate);
    let _ = fs::remove_file(snapshot.paths.rule_candidate(filename));
    if !snapshot.dir_existed {
      let _ = fs::remove_dir_all(&snapshot.paths.dir);
    }
  }

  fn rollback_snapshots(
    snapshots: &[InstanceSnapshot],
    filename: &str,
  ) -> IoResult<()> {
    let mut errors = Vec::new();

    for snapshot in snapshots {
      if let Err(err) = Self::stop_instance(&snapshot.paths) {
        errors.push(err.to_string());
      }
    }

    for snapshot in snapshots {
      let _ = fs::remove_file(&snapshot.paths.candidate);
      let _ = fs::remove_file(snapshot.paths.rule_candidate(filename));
      let _ = fs::remove_file(&snapshot.paths.config);
      let _ = fs::remove_file(snapshot.paths.rule(filename));
      if !snapshot.dir_existed {
        if let Err(err) = fs::remove_dir_all(&snapshot.paths.dir)
          && err.kind() != std::io::ErrorKind::NotFound
        {
          errors.push(err.to_string());
        }
        continue;
      }
      if let Err(err) = fs::create_dir_all(&snapshot.paths.rules) {
        errors.push(err.to_string());
        continue;
      }
      if let Some(config) = &snapshot.config
        && let Err(err) = fs::write(&snapshot.paths.config, config)
      {
        errors.push(err.to_string());
      }
      if let Some(rule) = &snapshot.rule
        && let Err(err) = fs::write(snapshot.paths.rule(filename), rule)
      {
        errors.push(err.to_string());
      }
    }

    for snapshot in snapshots {
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
    snapshots: &[InstanceSnapshot],
    filename: &str,
  ) -> IoError {
    match Self::rollback_snapshots(snapshots, filename) {
      Ok(()) => original,
      Err(rollback) => IoError::other(
        "dnsmasq rule".to_owned(),
        format!("{original}; rollback failed: {rollback}"),
      ),
    }
  }

  fn apply_rule_blocking(
    &self,
    key: &str,
    scope: &DnsmasqScope,
  ) -> IoResult<()> {
    let filename = Self::rule_filename(key)?;
    let target = self.paths_for(scope.listen_address);
    let old = self.find_rule_instance(&filename)?;
    let target_snapshot = Self::snapshot(&target, &filename)?;
    let mut snapshots = vec![target_snapshot.clone()];
    if let Some(old) = &old
      && old.dir != target.dir
    {
      snapshots.push(Self::snapshot(old, &filename)?);
    }

    let rule_config = Self::render_rule_config(scope)?;
    let instance_config =
      self.render_instance_config(scope.listen_address, &target)?;
    let root_changed =
      target_snapshot.config.as_deref() != Some(instance_config.as_str());

    let stage_result = (|| -> IoResult<()> {
      fs::create_dir_all(&target.rules).map_err(|err| {
        err.map_err_context(|| {
          format!("unable to create dnsmasq instance {}", target.dir.display())
        })
      })?;
      let _ = fs::remove_file(&target.candidate);
      let _ = fs::remove_file(target.rule_candidate(&filename));
      Self::stage_rule(&target, &filename, &rule_config)?;
      Self::validate_config(&target.rule_candidate(&filename))?;
      if root_changed {
        fs::write(&target.candidate, &instance_config).map_err(|err| {
          err.map_err_context(|| {
            format!(
              "unable to stage dnsmasq config {}",
              target.candidate.display()
            )
          })
        })?;
        Self::validate_config(&target.candidate)?;
      }
      Ok(())
    })();
    if let Err(err) = stage_result {
      Self::discard_staged(&target_snapshot, &filename);
      return Err(err);
    }

    let apply_result = (|| -> IoResult<bool> {
      for snapshot in &snapshots {
        if snapshot.was_running {
          Self::stop_instance(&snapshot.paths)?;
        }
      }
      if root_changed {
        fs::rename(&target.candidate, &target.config).map_err(|err| {
          err.map_err_context(|| {
            format!(
              "unable to commit dnsmasq config {}",
              target.config.display()
            )
          })
        })?;
      }
      Self::commit_rule(&target, &filename)?;
      let mut old_retired = false;
      if let Some(old) = &old
        && old.dir != target.dir
      {
        old_retired = !Self::remove_rule_file(old, &filename)?;
      }

      if !target.config.is_file() {
        return Err(IoError::invalid_data(
          "dnsmasq instance",
          &format!("missing dnsmasq config {}", target.config.display()),
        ));
      }
      Self::start_instance(&target)?;
      if let Some(old) = &old
        && old.dir != target.dir
        && !old_retired
      {
        if !old.config.is_file() {
          return Err(IoError::invalid_data(
            "dnsmasq instance",
            &format!("missing dnsmasq config {}", old.config.display()),
          ));
        }
        Self::start_instance(old)?;
      }
      Ok(old_retired)
    })();
    let old_retired = apply_result
      .map_err(|err| Self::rollback_error(err, &snapshots, &filename))?;

    for snapshot in &snapshots {
      let _ = fs::remove_file(&snapshot.paths.candidate);
      let _ = fs::remove_file(snapshot.paths.rule_candidate(&filename));
    }
    if old_retired
      && let Some(old) = &old
      && let Err(err) = fs::remove_dir_all(&old.dir)
    {
      log::warn!(
        "dnsmasq::rule: unable to clean unused instance {}: {err}",
        old.dir.display()
      );
    }
    Ok(())
  }

  fn remove_rule_blocking(&self, key: &str) -> IoResult<()> {
    let filename = Self::rule_filename(key)?;
    let Some(paths) = self.find_rule_instance(&filename)? else {
      return Ok(());
    };
    let snapshot = Self::snapshot(&paths, &filename)?;
    let snapshots = [snapshot];
    let apply_result = (|| -> IoResult<bool> {
      if snapshots[0].was_running {
        Self::stop_instance(&paths)?;
      }
      let has_rules = Self::remove_rule_file(&paths, &filename)?;
      if has_rules {
        if !paths.config.is_file() {
          return Err(IoError::invalid_data(
            "dnsmasq instance",
            &format!("missing dnsmasq config {}", paths.config.display()),
          ));
        }
        Self::validate_config(&paths.config)?;
        Self::start_instance(&paths)?;
      }
      Ok(!has_rules)
    })();
    let retired = apply_result
      .map_err(|err| Self::rollback_error(err, &snapshots, &filename))?;

    let _ = fs::remove_file(paths.rule_candidate(&filename));
    if retired {
      let _ = fs::remove_file(&paths.candidate);
      if let Err(err) = fs::remove_dir_all(&paths.dir) {
        log::warn!(
          "dnsmasq::rule: unable to clean unused instance {}: {err}",
          paths.dir.display()
        );
      }
    }
    Ok(())
  }

  /// Persist one Resource fragment and restart only its affected instance(s).
  pub(crate) async fn apply_rule(
    &self,
    key: &str,
    scope: DnsmasqScope,
  ) -> IoResult<()> {
    let _guard = self.update_lock.lock().await;
    let manager = self.clone();
    let key = key.to_owned();
    web::block(move || manager.apply_rule_blocking(&key, &scope)).await?;
    Ok(())
  }

  /// Remove one Resource fragment and preserve every other Resource file.
  pub(crate) async fn remove_rule(&self, key: &str) -> IoResult<()> {
    let _guard = self.update_lock.lock().await;
    let manager = self.clone();
    let key = key.to_owned();
    web::block(move || manager.remove_rule_blocking(&key)).await?;
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
      rules: Path::new(&self.config_dir).join("rules"),
    };
    if legacy.pid.is_file() {
      Self::stop_instance(&legacy)?;
    }
    Ok(self.clone())
  }

  /// Start all persisted instances. Normally rule application creates them;
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

  fn test_manager(name: &str) -> (PathBuf, Dnsmasq) {
    let root = std::env::temp_dir().join(format!(
      "ncdns-resource-rules-{}-{name}",
      std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let manager = Dnsmasq::new(root.to_str().unwrap());
    manager.ensure().unwrap();
    (root, manager)
  }

  fn commit_fragment(
    paths: &InstancePaths,
    key: &str,
    content: &str,
  ) -> String {
    let filename = Dnsmasq::rule_filename(key).unwrap();
    Dnsmasq::stage_rule(paths, &filename, content).unwrap();
    Dnsmasq::commit_rule(paths, &filename).unwrap();
    filename
  }

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
  fn resource_filenames_preserve_safe_names_and_reject_traversal() {
    for (name, filename) in [
      ("foo", "foo.conf"),
      ("internal-dns", "internal-dns.conf"),
      ("example.com", "example.com.conf"),
    ] {
      assert_eq!(Dnsmasq::rule_filename(name).unwrap(), filename);
    }
    for name in [
      "",
      ".",
      "..",
      ".hidden",
      "../foo",
      "dir/foo",
      "dir\\foo",
      "nul\0name",
    ] {
      assert!(Dnsmasq::rule_filename(name).is_err(), "{name:?} must fail");
    }
  }

  #[test]
  fn instance_and_resource_configuration_are_separate() {
    let manager =
      Dnsmasq::new("/tmp/ncdns-test").with_dns(vec!["1.1.1.1".to_owned()]);
    let mut scope = DnsmasqScope::new("172.30.0.1".parse().unwrap());
    scope.add_entry("api.internal".to_owned(), "172.30.0.2".parse().unwrap());
    scope.add_entry("api.internal".to_owned(), "172.30.0.2".parse().unwrap());
    let paths = manager.paths_for(scope.listen_address);
    let root = manager
      .render_instance_config(scope.listen_address, &paths)
      .unwrap();
    let rule = Dnsmasq::render_rule_config(&scope).unwrap();

    assert!(root.contains("listen-address=172.30.0.1"));
    assert!(
      root.contains(&format!("conf-dir={},*.conf", paths.rules.display()))
    );
    assert!(!root.contains("address=/"));
    assert!(!root.contains("interface=nanoclbr0"));
    assert_eq!(rule, "address=/api.internal/172.30.0.2\n");
  }

  #[test]
  fn invalid_names_cannot_inject_dnsmasq_directives() {
    let mut scope = DnsmasqScope::new("127.0.0.1".parse().unwrap());
    scope.add_entry("ok\nserver=evil".to_owned(), "127.0.0.1".parse().unwrap());
    assert!(Dnsmasq::render_rule_config(&scope).is_err());
  }

  #[test]
  fn adding_b_does_not_remove_a_fragment() {
    let (root, manager) = test_manager("add-b");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let a = commit_fragment(&paths, "resource-a", "address=/a.test/10.0.0.2\n");
    let b = commit_fragment(&paths, "resource-b", "address=/b.test/10.0.0.3\n");

    assert_eq!(
      fs::read_to_string(paths.rule(&a)).unwrap(),
      "address=/a.test/10.0.0.2\n"
    );
    assert!(paths.rule(&b).is_file());
    assert_eq!(Dnsmasq::rule_fragments(&paths).unwrap().len(), 2);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn updating_b_does_not_modify_a_fragment() {
    let (root, manager) = test_manager("update-b");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let a = commit_fragment(&paths, "resource-a", "address=/a.test/10.0.0.2\n");
    let b = commit_fragment(&paths, "resource-b", "address=/b.test/10.0.0.3\n");
    let original_a = fs::read(paths.rule(&a)).unwrap();

    Dnsmasq::stage_rule(&paths, &b, "address=/b.test/10.0.0.4\n").unwrap();
    Dnsmasq::commit_rule(&paths, &b).unwrap();

    assert_eq!(fs::read(paths.rule(&a)).unwrap(), original_a);
    assert_eq!(
      fs::read_to_string(paths.rule(&b)).unwrap(),
      "address=/b.test/10.0.0.4\n"
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn deleting_b_leaves_a_and_the_shared_instance() {
    let (root, manager) = test_manager("delete-b");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let a = commit_fragment(&paths, "resource-a", "address=/a.test/10.0.0.2\n");
    let b = commit_fragment(&paths, "resource-b", "address=/b.test/10.0.0.3\n");
    fs::write(&paths.config, "root").unwrap();

    assert!(Dnsmasq::remove_rule_file(&paths, &b).unwrap());

    assert!(paths.rule(&a).is_file());
    assert!(!paths.rule(&b).exists());
    assert!(paths.config.is_file());
    assert!(Dnsmasq::has_rule_fragments(&paths).unwrap());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn moving_a_resource_removes_its_old_fragment() {
    let (root, manager) = test_manager("move-resource");
    let old = manager.paths_for("10.0.0.1".parse().unwrap());
    let target = manager.paths_for("10.0.1.1".parse().unwrap());
    let filename =
      commit_fragment(&old, "resource-b", "address=/b.test/10.0.0.3\n");
    fs::write(&old.config, "old root").unwrap();
    assert_eq!(
      manager.find_rule_instance(&filename).unwrap().unwrap().dir,
      old.dir
    );

    commit_fragment(&target, "resource-b", "address=/b.test/10.0.1.3\n");
    assert!(!Dnsmasq::remove_rule_file(&old, &filename).unwrap());

    assert_eq!(
      manager.find_rule_instance(&filename).unwrap().unwrap().dir,
      target.dir
    );
    assert!(!old.rule(&filename).exists());
    assert!(!old.config.exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn final_resource_fragment_controls_instance_ownership() {
    let (root, manager) = test_manager("instance-ownership");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let a = commit_fragment(&paths, "resource-a", "");
    let b = commit_fragment(&paths, "resource-b", "");
    fs::write(&paths.config, "root").unwrap();

    assert!(Dnsmasq::remove_rule_file(&paths, &b).unwrap());
    assert!(Dnsmasq::has_rule_fragments(&paths).unwrap());
    assert!(paths.config.is_file());
    assert!(!Dnsmasq::remove_rule_file(&paths, &a).unwrap());
    assert!(!paths.config.exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn deleting_the_final_resource_removes_its_instance() {
    let (root, manager) = test_manager("remove-final-instance");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    commit_fragment(&paths, "resource-a", "");
    fs::write(&paths.config, "root").unwrap();

    manager.remove_rule_blocking("resource-a").unwrap();

    assert!(!paths.dir.exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn startup_discovers_persisted_instance_with_rules_directory() {
    let (root, manager) = test_manager("startup");
    let address = "10.0.0.1".parse().unwrap();
    let paths = manager.paths_for(address);
    commit_fragment(&paths, "resource-a", "address=/a.test/10.0.0.2\n");
    let config = manager.render_instance_config(address, &paths).unwrap();
    fs::write(&paths.config, &config).unwrap();

    let restarted = Dnsmasq::new(root.to_str().unwrap());
    let instances = restarted.configured_instances().unwrap();

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].dir, paths.dir);
    assert!(
      config.contains(&format!("conf-dir={},*.conf", paths.rules.display()))
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn persisted_fragments_are_discovered_without_runtime_rule_state() {
    let (root, manager) = test_manager("persisted-discovery");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let filename = commit_fragment(&paths, "resource-a", "");
    drop(manager);

    let restarted = Dnsmasq::new(root.to_str().unwrap());

    assert_eq!(
      restarted
        .find_rule_instance(&filename)
        .unwrap()
        .unwrap()
        .dir,
      paths.dir
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn rollback_restores_only_the_target_resource_fragment() {
    let (root, manager) = test_manager("rollback");
    let paths = manager.paths_for("10.0.0.1".parse().unwrap());
    let a = commit_fragment(&paths, "resource-a", "address=/a.test/10.0.0.2\n");
    let b = commit_fragment(&paths, "resource-b", "address=/b.test/10.0.0.3\n");
    fs::write(&paths.config, "old root").unwrap();
    let snapshot = Dnsmasq::snapshot(&paths, &b).unwrap();
    let original_a = fs::read(paths.rule(&a)).unwrap();
    fs::write(&paths.config, "new root").unwrap();
    fs::write(paths.rule(&b), "address=/b.test/10.0.0.4\n").unwrap();

    Dnsmasq::rollback_snapshots(&[snapshot], &b).unwrap();

    assert_eq!(fs::read_to_string(&paths.config).unwrap(), "old root");
    assert_eq!(
      fs::read_to_string(paths.rule(&b)).unwrap(),
      "address=/b.test/10.0.0.3\n"
    );
    assert_eq!(fs::read(paths.rule(&a)).unwrap(), original_a);
    fs::remove_dir_all(root).unwrap();
  }
}
