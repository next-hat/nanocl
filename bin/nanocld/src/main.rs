#[macro_use]
extern crate diesel;

use clap::Parser;

mod cli;
mod schema;
mod models;
mod version;
mod node;
mod boot;
mod utils;
mod event;
mod config;
mod server;
mod services;
mod repositories;

/// ## The Nanocl daemon
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
  log::info!(
    "nanocld_{}_{}_v{}:{}",
    version::ARCH,
    version::CHANNEL,
    version::VERSION,
    version::COMMIT_ID
  );
  let config = match config::init(&args) {
    Err(err) => {
      log::error!("{err}");
      std::process::exit(1);
    }
    Ok(config) => config,
  };
  // Boot and init internal dependencies
  let daemon_state = boot::init(&config).await?;
  if let Err(err) = node::join_cluster(&daemon_state).await {
    log::error!("{err}");
    std::process::exit(1);
  }
  node::register(&daemon_state).await?;
  utils::proxy::spawn_logger(&daemon_state);
  utils::metric::spawn_logger(&daemon_state);
  match server::gen(daemon_state).await {
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
