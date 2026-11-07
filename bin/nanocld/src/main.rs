/* Fix compilation error with ntex see (https://github.com/ntex-rs/ntex/issues/460) */
#![recursion_limit = "256"]

use clap::Parser;

use nanocl_error::io::FromIo;
use nanocl_utils::logger;

mod cli;
mod config;
mod models;
mod objects;
mod repositories;
mod schema;
mod services;
mod system;
mod tasks;
mod utils;
mod vars;

/// Provides an api to manage containers and virtual machines across physical hosts
/// There are these advantages :
/// - It's Open source
/// - It's Easy to use
/// - It keep an history of all your containers and virtual machines
#[ntex::main]
async fn main() -> std::io::Result<()> {
  // Parse command line arguments
  let args = cli::Cli::parse();
  logger::enable_logger(env!("CARGO_PKG_NAME"));
  log::info!(
    "nanocld_{}_v{}-{}:{}",
    vars::ARCH,
    vars::VERSION,
    vars::CHANNEL,
    vars::COMMIT_ID
  );
  // Init config by comparing command line arguments and config file
  let config = match config::init(&args) {
    Err(err) => {
      err.print_and_exit();
    }
    Ok(config) => config,
  };
  // Keep the exclusive gate for the process lifetime, including runtime teardown
  // after the HTTP server stops. The kernel closes it when the process exits.
  let daemon_guard =
    match utils::operation::acquire_daemon_guard(&config.state_dir) {
      Ok(guard) => guard,
      Err(error) => error.print_and_exit(),
    };
  std::mem::forget(daemon_guard);
  // Boot internal dependencies (database, event bus, etc...)
  let (daemon_state, start_crond) = match system::init(&config).await {
    Err(err) => {
      err.print_and_exit();
    }
    Ok(initialized) => initialized,
  };
  // Start http server
  match utils::server::generate(&daemon_state).await {
    Err(err) => {
      err.map_err_context(|| "Http server").print_and_exit();
    }
    Ok(server) => {
      // Listeners are bound and accepting; @reboot requests can now queue
      // while HTTP workers finish starting.
      start_crond();
      // Start http server and wait for shutdown
      // Server should never shutdown unless it's explicitly asked
      if let Err(err) = server.await {
        err.map_err_context(|| "Http server").print_and_exit();
      }
    }
  }
  log::info!("Node {} shutdown", daemon_state.inner.config.hostname);
  Ok(())
}
