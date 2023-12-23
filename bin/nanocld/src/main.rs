use clap::Parser;

use nanocl_error::io::FromIo;
use nanocl_utils::logger;

mod cli;
mod config;
mod schema;
mod version;
mod boot;
mod models;
mod event_emitter;
mod server;
mod repositories;
mod utils;
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
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    std::env::set_var("LOG_LEVEL", "nanocld=trace");
  }
  logger::enable_logger("nanocld");
  log::info!(
    "nanocld_{}_v{}-{}:{}",
    version::ARCH,
    version::VERSION,
    version::CHANNEL,
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
  let daemon_state = match boot::init(&config).await {
    Err(err) => {
      err.print_and_exit();
    }
    Ok(daemon_state) => daemon_state,
  };
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
  log::info!("main: shutdown");
  Ok(())
}
