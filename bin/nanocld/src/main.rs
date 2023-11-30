#[macro_use]
extern crate diesel;

use clap::Parser;
use nanocl_error::io::FromIo;

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

/// Provides an api to manage containers and virtual machines accross physical hosts
/// There are these advantages :
/// - It's Opensource
/// - It's Easy to use
/// - It keep an history of all your containers and virtual machines
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
  // Init config by comparing command line arguments and config file
  let config = match config::init(&args) {
    Err(err) => {
      err.print_and_exit();
    }
    Ok(config) => config,
  };
  // Boot internal dependencies (database, event bus, etc...)
  let daemon_state = boot::init(&config).await?;
  if let Err(err) = node::join_cluster(&daemon_state).await {
    err.print_and_exit();
  }
  // Register node to the cluster
  node::register(&daemon_state).await?;
  // Spawn proxy logger and metric logger
  utils::proxy::spawn_logger(&daemon_state);
  utils::metric::spawn_logger(&daemon_state);
  // Start http server
  match server::gen(daemon_state).await {
    Err(err) => {
      err.map_err_context(|| "Daemon state").print_and_exit();
    }
    Ok(server) => {
      // Start http server and wait for shutdown
      // Server should never shutdown unless it's explicitly asked
      if let Err(err) = server.await {
        err.map_err_context(|| "Http server").print_and_exit();
      }
    }
  }
  log::info!("shutdown");
  Ok(())
}
