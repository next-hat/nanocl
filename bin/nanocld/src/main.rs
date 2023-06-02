#[macro_use]
extern crate diesel;

use std::fs;
use std::path::Path;
use std::os::unix::prelude::PermissionsExt;

use ntex::rt;

use clap::Parser;

mod cli;
mod version;

mod node;
mod subsystem;
mod utils;
mod event;
mod schema;
mod models;
mod config;
mod server;
mod services;
mod repositories;

use notify::{Config, Watcher, RecursiveMode, RecommendedWatcher};

async fn set_unix_permission() {
  rt::Arbiter::new().exec_fn(|| {
    log::debug!("set_unix_permission");
    let path = Path::new("/run/nanocl");
    if !path.exists() {
      log::debug!(
        "{} doesn't exists cannot change unix socket permission",
        path.display()
      );
      return;
    }
    let (tx, rx) = std::sync::mpsc::channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
      Ok(watcher) => watcher,
      Err(e) => {
        log::warn!("watcher error: {:?}", e);
        return;
      }
    };

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path, RecursiveMode::Recursive).unwrap();

    log::debug!("watching change of: {}", path.display());
    for res in rx {
      match res {
        Ok(event) => {
          log::debug!("event: {:?}", event);
          if event.kind.is_modify() || event.kind.is_create() {
            log::debug!(
              "change detected, change permission of {}",
              path.display()
            );
            let mut perms = match fs::metadata("/run/nanocl/nanocl.sock") {
              Err(_) => {
                continue;
              }
              Ok(perms) => perms.permissions(),
            };
            #[cfg(feature = "dev")]
            {
              perms.set_mode(0o777);
              fs::set_permissions("/run/nanocl/nanocl.sock", perms).unwrap();
            }
            #[cfg(not(feature = "dev"))]
            {
              perms.set_mode(0o770);
              fs::set_permissions("/run/nanocl/nanocl.sock", perms).unwrap();
            }
            break;
          }
        }
        Err(err) => {
          log::warn!("watch error: {err:?}");
          break;
        }
      }
    }
    log::debug!("set_unix_permission done");
  });
}

/// # The Nanocl daemon
///
/// Provides an api to manage containers and virtual machines accross physical hosts
/// There are these advantages :
/// - It's Opensource
/// - It's Easy to use
/// - It keep an history of all your containers and virtual machines
///
#[ntex::main]
async fn main() -> std::io::Result<()> {
  // Parse command line arguments
  let args = cli::Cli::parse();

  // Build env logger
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", "nanocld=info,warn,error,nanocld=debug");
  }
  env_logger::Builder::new()
    .parse_env("LOG_LEVEL")
    .format_target(false)
    .init();

  let config = match config::init(&args) {
    Err(err) => {
      log::error!("{err}");
      std::process::exit(1);
    }
    Ok(config) => config,
  };

  // Boot and init internal dependencies
  let daemon_state = subsystem::init(&config).await?;

  // If init is true we don't start the server
  if args.init {
    return Ok(());
  }

  if let Err(err) = node::join_cluster(&daemon_state).await {
    log::error!("{err}");
    std::process::exit(1);
  }

  set_unix_permission().await;
  node::register(&daemon_state).await?;
  utils::proxy::spawn_logger(&daemon_state);
  utils::metric::spawn_logger(&daemon_state);

  match server::generate(daemon_state).await {
    Err(err) => {
      log::error!("Error while generating server {err}");
      std::process::exit(1);
    }
    Ok(server) => {
      // Start http server and wait for shutdown
      // Server should never shutdown unless it's explicitly asked
      if let Err(err) = server.await {
        log::error!("Error while running server {err}");
        std::process::exit(1);
      }
    }
  }
  log::info!("shutdown");
  Ok(())
}
