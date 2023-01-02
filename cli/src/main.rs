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

fn process_error(args: &Cli, err: CliError) {
  match err {
    CliError::Client(err) => match err {
      nanocl_client::error::NanoclClientError::SendRequest(err) => match err {
        ntex::http::client::error::SendRequestError::Connect(_) => {
          eprintln!(
            "Cannot connect to the nanocl daemon at {host}. Is the nanocl daemon running?",
            host = args.host
          )
        }
        _ => eprintln!("{}", err),
      },
      nanocl_client::error::NanoclClientError::Api(err) => {
        eprintln!("Daemon [{}]: {}", err.status, err.msg);
      }
      _ => eprintln!("{}", err),
    },
    _ => eprintln!("{}", err),
  }
  std::process::exit(1);
}

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
    process_error(&args, err);
  }
  Ok(())
}
