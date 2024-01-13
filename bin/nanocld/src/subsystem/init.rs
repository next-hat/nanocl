use std::{path::Path, process::Command, os::unix::prelude::PermissionsExt};

use ntex::rt;
use tokio::fs;
use notify::{Config, Watcher, RecursiveMode, RecommendedWatcher};

use nanocl_error::io::{FromIo, IoResult, IoError};
use nanocl_stubs::config::DaemonConfig;

use crate::{utils, models::SystemState};

/// Create a new thread and watch for change in the run directory
/// and set the permission of the unix socket
/// Then close the thread
fn set_uds_perm() {
  log::trace!("boot::set_uds_perm: start thread");
  rt::Arbiter::new().exec_fn(|| {
    rt::spawn(async {
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
      watcher.watch(path, RecursiveMode::Recursive).unwrap();
      log::trace!("boot::set_uds_perm: watching /run/nanocl");
      for res in rx {
        match res {
          Ok(event) => {
            if event.kind.is_modify()
              || event.kind.is_create()
              || event.kind.is_access()
              || event.kind.is_other()
            {
              log::trace!("boot::set_uds_perm: /run/nanocl change detected",);
              let mut perms =
                match fs::metadata("/run/nanocl/nanocl.sock").await {
                  Err(err) => {
                    log::warn!(
                      "boot::set_uds_perm: /run/nanocl/nanocl.sock {err}"
                    );
                    break;
                  }
                  Ok(perms) => perms.permissions(),
                };
              perms.set_mode(0o770);
              if let Err(err) =
                fs::set_permissions("/run/nanocl/nanocl.sock", perms).await
              {
                log::warn!("boot::set_uds_perm: /run/nanocl/nanocl.sock {err}");
              }
              log::trace!(
                "boot::set_uds_perm: /run/nanocl/nanocl.sock permission set"
              );
              break;
            }
          }
          Err(err) => {
            log::warn!("boot::set_uds_perm: watcher {err}");
            break;
          }
        }
      }
      log::trace!("boot::set_uds_perm: stop thread");
      rt::Arbiter::current().stop();
    });
  });
}

/// Create a new thread and spawn and manage a crond instance to run cron jobs
fn spawn_crond() {
  log::trace!("boot::spawn_crond: start thread");
  rt::Arbiter::new().exec_fn(|| {
    rt::spawn(async {
      let task = ntex::web::block(move || {
        match Command::new("crond").args(["-f"]).spawn() {
          Ok(mut child) => {
            child.wait()?;
            Ok(())
          }
          Err(err) => Err(err),
        }
      })
      .await;
      if let Err(err) = task {
        log::error!("boot::spawn_crond: {err}");
      }
      log::trace!("boot::spawn_crond: stop thread");
      rt::Arbiter::current().stop();
    });
  });
}

/// Ensure that the state dir exists and is ready to use
async fn ensure_state_dir(state_dir: &str) -> IoResult<()> {
  let vm_dir = format!("{state_dir}/vms/images");
  fs::create_dir_all(vm_dir).await.map_err(|err| {
    err.map_err_context(|| format!("Unable to create {state_dir}/vms/images"))
  })?;
  Ok(())
}

/// Init function called before http server start.
/// To boot and initialize our state and database.
pub async fn init(conf: &DaemonConfig) -> IoResult<SystemState> {
  spawn_crond();
  set_uds_perm();
  ensure_state_dir(&conf.state_dir).await?;
  let system_state = SystemState::new(conf).await?;
  let system_ptr = system_state.clone();
  utils::node::register(&system_ptr).await?;
  utils::system::register_namespace("global", true, &system_ptr).await?;
  utils::system::register_namespace("system", false, &system_ptr).await?;
  rt::spawn(async move {
    let fut = async move {
      utils::system::sync_processes(&system_ptr).await?;
      utils::system::sync_vm_images(&system_ptr).await?;
      Ok::<_, IoError>(())
    };
    if let Err(err) = fut.await {
      log::warn!("boot::init: {err}");
    }
    Ok::<_, IoError>(())
  });
  super::docker_event::analyze(&system_state);
  super::event::analyze(&system_state);
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
      hosts: None,
      docker_host: None,
      state_dir: Some(format!("{home}/.nanocl_dev/state")),
      conf_dir: String::from("/etc/nanocl"),
      gateway: None,
      nodes: Vec::default(),
      hostname: None,
      advertise_addr: None,
    };
    log::debug!("args: {args:?}");
    let config = config::init(&args).expect("Expect to init config");
    log::debug!("config: {config:?}");
    // Test state
    let state = init(&config).await.unwrap();
    let state_ptr = state.clone();
    let mut raw_sub = state.subscribe_raw().unwrap();
    rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      let actor = Resource::default();
      state_ptr.emit_normal_native_action(
        &actor,
        nanocl_stubs::system::NativeEventAction::Create,
      );
    });
    raw_sub.next().await;
    let state_ptr = state.clone();
    rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      let actor = Resource::default();
      state_ptr.emit_normal_native_action(
        &actor,
        nanocl_stubs::system::NativeEventAction::Create,
      );
    });
    let mut sub = state.subscribe().await.unwrap();
    sub.next().await;
  }
}
