use ntex::http;
use clap::Parser;
use dotenv::dotenv;

use nanocl_utils::io_error::IoResult;
use nanocl_utils::http_client_error::HttpClientError;

use nanocld_client::NanocldClient;

mod utils;
mod config;
mod models;
mod version;
mod commands;

use models::{Cli, Commands};

async fn detect_version(client: &mut NanocldClient) -> IoResult<()> {
  client.set_version("0.1.0");
  let version = client.get_version().await?;
  client.set_version(&version.version);

  Ok(())
}

async fn execute_args(args: &Cli) -> IoResult<()> {
  let cli_conf = config::read();

  #[allow(unused)]
  let mut host = args.host.clone().unwrap_or(cli_conf.host);
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    host = args.host.clone().unwrap_or("http://localhost:8585".into());
  }

  let url = Box::leak(host.clone().into_boxed_str());
  let mut client = NanocldClient::connect_to(url, None);

  if let Err(HttpClientError::HttpError(err)) = client.get_version().await {
    if err.status == http::StatusCode::NOT_FOUND {
      eprintln!("You're daemon is outdated, please run `nanocl upgrade`");
      detect_version(&mut client).await?;
    }
  }

  match &args.command {
    Commands::Namespace(args) => commands::exec_namespace(&client, args).await,
    Commands::Resource(args) => commands::exec_resource(&client, args).await,
    Commands::Cargo(args) => commands::exec_cargo(&client, args).await,
    Commands::Events => commands::exec_events(&client).await,
    Commands::State(args) => commands::exec_state(&host, args).await,
    Commands::Version => commands::exec_version(&client).await,
    Commands::Info => commands::exec_info(&client).await,
    Commands::Vm(args) => commands::exec_vm(&client, args).await,
    Commands::Ps(opts) => commands::exec_process(&client, opts).await,
    Commands::Install(opts) => commands::exec_install(opts).await,
    Commands::Uninstall(opts) => commands::exec_uninstall(opts).await,
    Commands::Upgrade(opts) => commands::exec_upgrade(&client, opts).await,
    Commands::System(opts) => commands::exec_system(&client, opts).await,
    Commands::Node(args) => commands::exec_node(&client, args).await,
  }
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let args = Cli::parse();
  dotenv().ok();
  ctrlc::set_handler(move || {
    let term = dialoguer::console::Term::stdout();
    let _ = term.show_cursor();
    let _ = term.clear_last_lines(1);
    std::process::exit(0);
  })
  .expect("Error setting Ctrl-C handler");
  if let Err(err) = execute_args(&args).await {
    eprintln!("{err}");
    err.exit();
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  use nanocld_client::NanocldClient;

  /// Test version command
  #[ntex::test]
  async fn version() {
    let args = Cli::parse_from(["nanocl", "version"]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Namespace commands
  #[ntex::test]
  async fn namespace() {
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
    let args =
      Cli::parse_from(["nanocl", "namespace", "rm", "-y", NAMESPACE_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Cargo image commands
  #[ntex::test]
  async fn cargo_image() {
    const IMAGE_NAME: &str = "busybox:1.26.0";
    // Try to create cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "pull", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to list cargo images
    let args = Cli::parse_from(["nanocl", "cargo", "image", "ls"]);
    assert!(execute_args(&args).await.is_ok());
    // Try to inspect cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "inspect", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "rm", "-y", IMAGE_NAME]);
    assert!(execute_args(&args).await.is_ok());
    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "image",
      "import",
      "-f",
      "../../tests/busybox.tar.gz",
    ]);
    let res = execute_args(&args).await;
    assert!(res.is_ok());
  }

  /// Test Cargo commands
  #[ntex::test]
  async fn cargo() {
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
    let client = NanocldClient::connect_to("http://localhost:8585", None);
    let history = client
      .list_history_cargo(CARGO_NAME, None)
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();

    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "revert",
      CARGO_NAME,
      &history.key.to_string(),
    ]);
    assert!(execute_args(&args).await.is_ok());

    // Try to stop a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "stop", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
    // Try to remove cargo
    let args = Cli::parse_from(["nanocl", "cargo", "rm", "-y", CARGO_NAME]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test Resource commands
  #[ntex::test]
  async fn resource() {
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/basic_resources.yml",
    ]);
    // ensure that ProxyRule and DnsRule are available
    _ = execute_args(&args).await;

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());
    // Create a new resource
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/resource_ssl_example.yml",
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

    let client = NanocldClient::connect_to("http://localhost:8585", None);
    let history = client
      .list_history_resource("resource-example")
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();

    let args = Cli::parse_from([
      "nanocl",
      "resource",
      "revert",
      "resource-example",
      &history.key.to_string(),
    ]);
    assert!(execute_args(&args).await.is_ok());

    // Remove resource
    let args =
      Cli::parse_from(["nanocl", "resource", "rm", "-y", "resource-example"]);
    assert!(execute_args(&args).await.is_ok());
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());
  }

  /// Test cargo exec command
  #[ntex::test]
  async fn cargo_exec() {
    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "--",
      "echo",
      "hello",
    ]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn state() {
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.toml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-pys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn info() {
    let args = Cli::parse_from(["nanocl", "info"]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn cargo_run() {
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

    let args = Cli::parse_from(["nanocl", "cargo", "rm", "-y", "cli-test-run"]);
    assert!(execute_args(&args).await.is_ok());
  }

  #[ntex::test]
  async fn node_list() {
    let args = Cli::parse_from(["nanocl", "node", "ls"]);
    assert!(execute_args(&args).await.is_ok());
  }
}
