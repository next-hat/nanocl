use clap::Parser;
use nanocl_client::NanoclClient;

mod utils;
mod error;
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

#[cfg(test)]
mod tests {
  use super::*;

  /// Test version command
  #[ntex::test]
  async fn test_version() {
    let args = Cli::parse_from(["nanocl", "version"]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Namespace commands
  #[ntex::test]
  async fn test_namespace() {
    const NAMESPACE_NAME: &str = "cli-namespace";
    // Try to create namespace
    let args =
      Cli::parse_from(["nanocl", "namespace", "create", NAMESPACE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to list namespaces
    let args = Cli::parse_from(["nanocl", "namespace", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Try to inspect namespace
    let args =
      Cli::parse_from(["nanocl", "namespace", "inspect", NAMESPACE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove namespace
    let args = Cli::parse_from(["nanocl", "namespace", "rm", NAMESPACE_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Cargo image commands
  #[ntex::test]
  async fn test_cargo_image() {
    const IMAGE_NAME: &str = "busybox:1.26.0";
    // Try to create cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "create", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to list cargo images
    let args = Cli::parse_from(["nanocl", "cargo", "image", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Try to inspect cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "inspect", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove cargo image
    let args = Cli::parse_from(["nanocl", "cargo", "image", "rm", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Cargo commands
  #[ntex::test]
  async fn test_cargo() {
    const CARGO_NAME: &str = "cli-test";
    const IMAGE_NAME: &str = "nexthat/nanocl-get-started:latest";
    // Try to create cargo
    let args =
      Cli::parse_from(["nanocl", "cargo", "create", CARGO_NAME, IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to list cargos
    let args = Cli::parse_from(["nanocl", "cargo", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Try to start a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "start", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to stop a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "stop", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove cargo
    let args = Cli::parse_from(["nanocl", "cargo", "rm", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }
}
