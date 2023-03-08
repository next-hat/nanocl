#[macro_use]
extern crate diesel;

use clap::Parser;

mod cli;
mod version;

mod boot;
mod error;
mod utils;
mod event;
mod schema;
mod models;
mod config;
mod server;
mod metric;
mod services;
mod repositories;

/// # The Nanocl daemon
///
/// Provides an api to manage network and containers accross physical hosts
/// there are these advantages :
/// - It's Opensource
/// - It's Easy to use
/// - It keep an history of all your containers and networks
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
      log::error!("Error while initing config: {}", err.msg);
      std::process::exit(err.code);
    }
    Ok(config) => config,
  };

  // Boot and init internal dependencies
  let daemon_state = match boot::init(&config).await {
    Err(err) => {
      log::error!("Error while booting daemon {}", err.msg);
      std::process::exit(err.code);
    }
    Ok(state) => state,
  };

  // If init is true we don't start the server
  if args.init {
    return Ok(());
  }

  metric::spawn_metrics(&daemon_state.config.hostname, &daemon_state.pool);

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
