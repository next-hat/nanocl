use clap::Parser;
use nanocl_client::NanoclClient;

mod utils;
mod error;
mod config;
mod models;
mod version;
mod commands;

use error::CliError;
use models::{Cli, Commands};

async fn execute_args(args: &Cli) -> Result<(), CliError> {
  let client = NanoclClient::connect_with_unix_default().await;
  match &args.command {
    Commands::Setup(args) => commands::exec_setup(args).await,
    Commands::Namespace(args) => commands::exec_namespace(&client, args).await,
    Commands::Cargo(args) => commands::exec_cargo(&client, args).await,
    Commands::Version => commands::exec_version(&client).await,
  }
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let args = Cli::parse();
  if let Err(err) = execute_args(&args).await {
    err.exit(&args);
  }
  Ok(())
}
