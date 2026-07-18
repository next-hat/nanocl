use std::{
  os::unix::prelude::PermissionsExt, path::Path, process::Stdio, time::Duration,
};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ntex::rt;
use tokio::{
  fs,
  io::{AsyncBufReadExt, BufReader},
  process::Command as TokioCommand,
};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::generic::{GenericClause, GenericFilter};

use crate::{
  models::{DistributedMutexDb, NodeDb, SystemState},
  repositories::generic::RepositoryDelBy,
  utils,
};

async fn maybe_set_uds_perm() -> bool {
  let mut perms = match fs::metadata("/run/nanocl/nanocl.sock").await {
    Ok(metadata) => metadata.permissions(),
    Err(err) => {
      // The watcher can receive events before the daemon creates the socket.
      if err.kind() != std::io::ErrorKind::NotFound {
        log::warn!("boot::set_uds_perm: /run/nanocl/nanocl.sock {err}");
      }
      return false;
    }
  };
  perms.set_mode(0o770);
  if let Err(err) = fs::set_permissions("/run/nanocl/nanocl.sock", perms).await
  {
    log::warn!("boot::set_uds_perm: /run/nanocl/nanocl.sock {err}");
    return false;
  }
  log::trace!("boot::set_uds_perm: /run/nanocl/nanocl.sock permission set");
  true
}

/// Create a new thread and watch for change in the run directory
/// and set the permission of the unix socket
/// Then close the thread
///
fn set_uds_perm() {
  log::trace!("boot::set_uds_perm: start thread");
  rt::Arbiter::new().handle().spawn(async move {
    let path = Path::new("/run/nanocl");
    if !path.exists() {
      log::warn!("boot::set_uds_perm: /run/nanocl not found");
      return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
      Ok(watcher) => watcher,
      Err(e) => {
        log::warn!("boot::set_uds_perm: {e}");
        return;
      }
    };
    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    if let Err(err) = watcher.watch(path, RecursiveMode::Recursive) {
      log::warn!("boot::set_uds_perm: watcher {err}");
      return;
    }
    log::trace!("boot::set_uds_perm: watching /run/nanocl");

    // Fast path for cases where the socket already exists.
    if maybe_set_uds_perm().await {
      log::trace!("boot::set_uds_perm: stop thread");
      return;
    }

    for res in rx {
      match res {
        Ok(event) => {
          if event.kind.is_modify()
            || event.kind.is_create()
            || event.kind.is_access()
            || event.kind.is_other()
          {
            log::trace!("boot::set_uds_perm: /run/nanocl change detected",);
            if maybe_set_uds_perm().await {
              break;
            }
          }
        }
        Err(err) => {
          log::warn!("boot::set_uds_perm: watcher {err}");
          break;
        }
      }
    }
    log::trace!("boot::set_uds_perm: stop thread");
  });
}

/// Create a new thread and spawn and manage a crond instance to run cron jobs
///
fn spawn_crond() {
  log::trace!("boot::spawn_crond: start thread");
  rt::Arbiter::new().handle().spawn(async move {
    // Spawn crond in foreground with piped stdout/stderr
    match TokioCommand::new("crond")
      .args(["-f", "-d", "8", "-l", "8", "-L", "/dev/stdout"])
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
    {
      Ok(mut child) => {
        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
          rt::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
              match lines.next_line().await {
                Ok(Some(line)) => log::info!("{line}"),
                Ok(None) => break,
                Err(err) => {
                  log::warn!("crond stdout read error: {err}");
                  break;
                }
              }
            }
          });
        }
        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
          rt::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            loop {
              match lines.next_line().await {
                Ok(Some(line)) => log::warn!("{line}"),
                Ok(None) => break,
                Err(err) => {
                  log::warn!("crond stderr read error: {err}");
                  break;
                }
              }
            }
          });
        }
        // Wait for crond to exit
        match child.wait().await {
          Ok(status) => {
            log::info!("boot::spawn_crond: crond exited with {status}");
          }
          Err(err) => {
            log::error!("boot::spawn_crond: wait error: {err}");
          }
        }
      }
      Err(err) => {
        log::error!("boot::spawn_crond: spawn error: {err}");
      }
    }
    log::trace!("boot::spawn_crond: stop thread");
  });
}

/// Create a new thread and periodically delete expired distributed mutexes.
///
/// CockroachDB used TTL table options, but PostgreSQL does not.
/// We emulate TTL by running a delete loop every second.
fn spawn_distributed_mutexes_gc(state: &SystemState) {
  log::debug!("boot::spawn_distributed_mutexes_gc: start thread");
  let pool = state.inner.pool.clone();
  rt::Arbiter::new().handle().spawn(async move {
    loop {
      let filter = GenericFilter::new().r#where(
        "expires_at",
        GenericClause::Le(chrono::Utc::now().to_rfc3339()),
      );
      if let Err(err) = DistributedMutexDb::del_by(&filter, &pool).await {
        log::warn!("boot::spawn_distributed_mutexes_gc: {err}");
      }
      ntex::time::sleep(Duration::from_secs(1)).await;
    }
  });
}

/// Ensure that the state dir exists and is ready to use
///
async fn ensure_state_dir(state_dir: &str) -> IoResult<()> {
  fs::create_dir_all(format!("{state_dir}/secrets"))
    .await
    .map_err(|err| {
      err.map_err_context(|| format!("Unable to create {state_dir}/secrets"))
    })?;
  Ok(())
}

pub fn docker_healthcheck(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().handle().spawn(async move {
    loop {
      if let Err(err) = state.inner.docker_api.ping().await {
        log::error!(
          "boot::docker_healthcheck: Docker daemon not reachable: {err}"
        );
        let error = IoError::interrupted(
          "Docker Healthcheck",
          "Docker daemon not reachable",
        );
        error.print_and_exit();
      }
      ntex::time::sleep(std::time::Duration::from_secs(5)).await;
    }
  });
}

/// Init function called before http server start.
/// To boot and initialize our state and database.
///
pub async fn init(conf: &DaemonConfig) -> IoResult<SystemState> {
  spawn_crond();
  set_uds_perm();
  ensure_state_dir(&conf.state_dir).await?;
  let system_state = SystemState::new(conf).await?;
  docker_healthcheck(&system_state);
  spawn_distributed_mutexes_gc(&system_state);
  NodeDb::register(&system_state).await?;
  utils::container::network::reconcile_networks(&system_state).await?;
  utils::system::register_namespace("global", &system_state).await?;
  utils::system::register_namespace("system", &system_state).await?;
  let system_ptr = system_state.clone();
  rt::spawn(async move {
    let fut = async move {
      utils::system::sync_processes(&system_ptr).await?;
      Ok::<_, IoError>(())
    };
    if let Err(err) = fut.await {
      log::warn!("boot::init: {err}");
    }
    Ok::<_, IoError>(())
  });
  super::docker_event::analyze(&system_state);
  super::metric::spawn(&system_state);
  Ok(system_state)
}

/// Init unit test
#[cfg(test)]
mod tests {
  use futures_util::StreamExt;

  use nanocl_stubs::resource::Resource;

  use super::*;

  use crate::{cli, config, utils::tests::*};

  /// Test init
  #[ntex::test]
  async fn basic_init() {
    // Init cli args
    before();
    let home = std::env::var("HOME").expect("Failed to get home dir");
    let args = cli::Cli {
      gid: 0,
      state_dir: Some(format!("{home}/.nanocl_dev/state")),
      store_addr: Some(
        "postgresql://root:root@store.nanocl.internal:26258/defaultdb"
          .to_owned(),
      ),
      hostname: Some("init-test.nanocl.io".to_owned()),
      gateway: Some("127.0.0.1".to_owned()),
      conf_dir: String::from("/etc/nanocl"),
      nodes: Vec::default(),
      ..Default::default()
    };
    log::debug!("args: {args:?}");
    let config = config::init(&args).expect("Expect to init config");
    log::debug!("config: {config:?}");
    // Test state
    let state = init(&config).await.unwrap();
    let state_ptr = state.clone();
    let mut raw_sub = state.subscribe_raw(None).await.unwrap();
    rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      let actor = Resource::default();
      state_ptr
        .emit_normal_native_action_sync(
          &actor,
          nanocl_stubs::system::NativeEventAction::Create,
        )
        .await;
    });
    raw_sub.next().await;
    let state_ptr = state.clone();
    rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      let actor = Resource::default();
      state_ptr
        .emit_normal_native_action_sync(
          &actor,
          nanocl_stubs::system::NativeEventAction::Create,
        )
        .await;
    });
  }
}
