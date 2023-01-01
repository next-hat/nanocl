#[macro_use]
extern crate diesel;

use clap::Parser;

mod cli;
mod version;
mod controllers;

mod utils;
mod state;
mod errors;
mod schema;
mod models;
mod server;
mod openapi;
mod services;
mod repositories;

/// nanocl daemon
///
/// Provides an api to manage network and containers accross physical hosts
/// there are these advantages :
/// - Opensource
/// - Easy
#[ntex::main]
async fn main() -> std::io::Result<()> {
  // Parse command line arguments
  let args = cli::Cli::parse();

  // Build env logger
  if std::env::var("LOG_LEVEL").is_err() {
    std::env::set_var("LOG_LEVEL", "nanocld=info,warn,error,nanocld=debug");
  }
  env_logger::Builder::new().parse_env("LOG_LEVEL").init();

  // Init internal config and dependencies
  let daemon_state = match state::init(&args).await {
    Err(err) => {
      let exit_code = errors::parse_main_error(err);
      std::process::exit(exit_code);
    }
    Ok(state) => state,
  };

  // If init is true we don't start the server
  if args.init {
    return Ok(());
  }

  // start http server
  let srv = server::start(daemon_state).await?;
  srv.await?;
  log::info!("shutdown");
  Ok(())
}
