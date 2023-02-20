use clap::Parser;
use nanocld_client::NanocldClient;

mod utils;
mod error;
mod models;
mod version;
mod commands;

use error::CliError;
use models::{Cli, Commands};

async fn execute_args(args: &Cli) -> Result<(), CliError> {
  let client = NanocldClient::connect_with_unix_default();
  match &args.command {
    Commands::Namespace(args) => commands::exec_namespace(&client, args).await,
    Commands::Resource(args) => commands::exec_resource(&client, args).await,
    Commands::Cargo(args) => commands::exec_cargo(&client, args).await,
    Commands::Events => commands::exec_events(&client).await,
    Commands::State(args) => commands::exec_state(&client, args).await,
    Commands::Version(args) => commands::exec_version(&client, args).await,
    Commands::Info => commands::exec_info(&client).await,
    Commands::Setup(opts) => commands::exec_setup(opts).await,
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

  use nanocld_client::NanocldClient;
  use ntex::time::{interval, Seconds};

  /// Test version command
  #[ntex::test]
  async fn test_version() {
    let args = Cli::parse_from(["nanocl", "version"]);
    assert!(execute_args(&args).await.is_ok());
    // let args = Cli::parse_from(["nanocl", "version", "check"]);
    // assert!(execute_args(&args).await.is_ok());
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
    // Try to import a cargo image from a tarball
    // NOTE: It's bugging out in CI, but works locally
    // It's timeouting on the `client.import_image` call
    // let fp = std::env::current_dir()
    //   .unwrap()
    //   .join("../../tests/busybox.tar.gz");
    // let args = Cli::parse_from([
    //   "nanocl",
    //   "cargo",
    //   "image",
    //   "import",
    //   "-f",
    //   &fp.display().to_string(),
    // ]);
    // let res = execute_args(&args).await;
    // println!("res : {res:#?}");
    // assert!(res.is_ok());
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
    // Try to list cargoes
    let args = Cli::parse_from(["nanocl", "cargo", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Try to start a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "start", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to inspect a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "inspect", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to patch a cargo
    let args = Cli::parse_from([
      "nanocl", "cargo", "patch", CARGO_NAME, "--image", IMAGE_NAME, "--env",
      "TEST=1",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "history", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    let client = NanocldClient::connect_with_unix_default();
    let history = client
      .list_history_cargo(CARGO_NAME, None)
      .await
      .unwrap()
      .first()
      .unwrap()
      .to_owned();

    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "reset",
      CARGO_NAME,
      &history.key.to_string(),
    ]);
    assert!(execute_args(&args).await.is_ok());

    // Try to stop a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "stop", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove cargo
    let args = Cli::parse_from(["nanocl", "cargo", "rm", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Resource commands
  #[ntex::test]
  async fn test_resource() {
    // Create a new resource
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/resource_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());
    // List resources
    let args = Cli::parse_from(["nanocl", "resource", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Inspect resource
    let args =
      Cli::parse_from(["nanocl", "resource", "inspect", "resource-example"]);
    assert!(execute_args(&args).await.is_ok());

    // History
    let args =
      Cli::parse_from(["nanocl", "resource", "history", "resource-example"]);
    assert!(execute_args(&args).await.is_ok());

    let client = NanocldClient::connect_with_unix_default();
    let history = client
      .list_history_resource("resource-example")
      .await
      .unwrap()
      .first()
      .unwrap()
      .to_owned();

    let args = Cli::parse_from([
      "nanocl",
      "resource",
      "reset",
      "resource-example",
      &history.key.to_string(),
    ]);
    assert!(execute_args(&args).await.is_ok());

    // Remove resource
    let args =
      Cli::parse_from(["nanocl", "resource", "rm", "resource-example"]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test cargo exec command
  #[ntex::test]
  async fn test_cargo_exec() {
    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "store",
      "--",
      "echo",
      "hello",
    ]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn test_state() {
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "revert",
      "-f",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-f",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "revert",
      "-f",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn test_info() {
    let args = Cli::parse_from(["nanocl", "info"]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn test_setup() {
    let args = Cli::parse_from([
      "nanocl",
      "setup",
      "--deamon-hosts",
      "unix:///run/nanocl/nanocl2.sock",
      "--state-dir",
      "/tmp/nanocl2",
    ]);
    assert!(execute_args(&args).await.is_ok());

    // Wait before trying to stop the cargo
    interval(Seconds(4)).tick().await;

    let args =
      Cli::parse_from(["nanocl", "cargo", "-n", "system", "stop", "daemon"]);
    let _ = execute_args(&args).await;

    let args =
      Cli::parse_from(["nanocl", "cargo", "-n", "system", "rm", "daemon"]);
    let _ = execute_args(&args).await.is_ok();
  }

  #[ntex::test]
  async fn test_cargo_run() {
    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "run",
      "cli-test-run",
      "nexthat/nanocl-get-started",
      "-e",
      "MESSAGE=GREETING",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "stop", "cli-test-run"]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "rm", "cli-test-run"]);
    assert!(execute_args(&args).await.is_ok());
  }
}
