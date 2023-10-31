use clap::Parser;
use dotenv::dotenv;

use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

mod utils;
mod config;
mod models;
mod version;
mod commands;

use config::{UserConfig, CliConfig};
use models::{Cli, Command, Context};

/// ## Create cli config
///
/// Create a CliConfig struct from the cli arguments
///
/// ## Arguments
///
/// * [cli_args](Cli) The cli arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](CliConfig) The cli configuration
///   * [Err](nanocl_error::io::IoError) The error of the operation
///
fn create_cli_config(cli_args: &Cli) -> IoResult<CliConfig> {
  Context::ensure()?;
  let user_conf = UserConfig::new();
  let mut context = Context::new();
  if user_conf.current_context != "default" {
    match Context::read_by_name(&user_conf.current_context) {
      Err(_) => {
        Context::r#use("default")?;
      }
      Ok(cur_context) => {
        context = cur_context;
      }
    }
  }
  #[allow(unused)]
  let mut host = cli_args
    .host
    .clone()
    .unwrap_or(context.endpoints.get("Nanocl").unwrap().host.clone());
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    if context.name == "default" {
      host = cli_args
        .host
        .clone()
        .unwrap_or("http://ndaemon.nanocl.internal:8585".into());
    }
  }
  let url = Box::leak(host.clone().into_boxed_str());
  let client = NanocldClient::connect_to(url, None);
  Ok(CliConfig {
    host,
    client,
    context,
    user_config: user_conf,
  })
}

/// ## Execute arg
///
/// Execute the command from the cli arguments
///
/// ## Arguments
///
/// * [cli_args](Cli) The cli arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) The error of the operation
///
async fn execute_arg(cli_args: &Cli) -> IoResult<()> {
  let cli_conf = create_cli_config(cli_args)?;
  match &cli_args.command {
    Command::Namespace(args) => commands::exec_namespace(&cli_conf, args).await,
    Command::Resource(args) => commands::exec_resource(&cli_conf, args).await,
    Command::Cargo(args) => commands::exec_cargo(&cli_conf, args).await,
    Command::Secret(args) => commands::exec_secret(&cli_conf, args).await,
    Command::Events => commands::exec_events(&cli_conf).await,
    Command::State(args) => commands::exec_state(&cli_conf, args).await,
    Command::Version => commands::exec_version(&cli_conf).await,
    Command::Vm(args) => commands::exec_vm(&cli_conf, args).await,
    Command::Ps(args) => commands::exec_process(&cli_conf, args).await,
    Command::Install(args) => commands::exec_install(args).await,
    Command::Uninstall(args) => commands::exec_uninstall(args).await,
    Command::Upgrade(args) => commands::exec_upgrade(&cli_conf, args).await,
    Command::System(args) => commands::exec_system(&cli_conf, args).await,
    Command::Node(args) => commands::exec_node(&cli_conf, args).await,
    Command::Context(args) => commands::exec_context(&cli_conf, args).await,
    Command::Info => commands::exec_info(&cli_conf).await,
  }
}

/// ## Main
///
/// Nanocl is a command line interface for the Nanocl Daemon.
/// It will translate the conresponding commands to the Nanocl Daemon API.
/// You can use it to manage your cargoes and virtual machines.
///
#[ntex::main]
async fn main() -> std::io::Result<()> {
  let args = Cli::parse();
  dotenv().ok();
  ctrlc::set_handler(move || {
    let term = dialoguer::console::Term::stdout();
    let _ = term.show_cursor();
    let _ = term.clear_last_lines(0);
    std::process::exit(0);
  })
  .expect("Error setting Ctrl-C handler");
  if let Err(err) = execute_arg(&args).await {
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
    assert!(execute_arg(&args).await.is_ok());
  }

  /// Test Namespace commands
  #[ntex::test]
  async fn namespace() {
    const NAMESPACE_NAME: &str = "cli-namespace";
    // Try to create namespace
    let args =
      Cli::parse_from(["nanocl", "namespace", "create", NAMESPACE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to list namespaces
    let args = Cli::parse_from(["nanocl", "namespace", "ls"]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to inspect namespace
    let args =
      Cli::parse_from(["nanocl", "namespace", "inspect", NAMESPACE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to remove namespace
    let args =
      Cli::parse_from(["nanocl", "namespace", "rm", "-y", NAMESPACE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
  }

  /// Test Cargo image commands
  #[ntex::test]
  async fn cargo_image() {
    const IMAGE_NAME: &str = "busybox:1.26.0";
    // Try to create cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "pull", IMAGE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to list cargo images
    let args = Cli::parse_from(["nanocl", "cargo", "image", "ls"]);
    let res = execute_arg(&args).await;

    println!("{:?}", res);
    assert!(res.is_ok());
    // Try to inspect cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "inspect", IMAGE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to remove cargo image
    let args =
      Cli::parse_from(["nanocl", "cargo", "image", "rm", "-y", IMAGE_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    let args = Cli::parse_from([
      "nanocl",
      "cargo",
      "image",
      "import",
      "-f",
      "../../tests/busybox.tar.gz",
    ]);
    let res = execute_arg(&args).await;
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
    assert!(execute_arg(&args).await.is_ok());
    // Try to list cargoes
    let args = Cli::parse_from(["nanocl", "cargo", "ls"]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to start a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "start", CARGO_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to inspect a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "inspect", CARGO_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to patch a cargo
    let args = Cli::parse_from([
      "nanocl", "cargo", "patch", CARGO_NAME, "--image", IMAGE_NAME, "--env",
      "TEST=1",
    ]);
    let ret = execute_arg(&args).await;

    println!("{:?}", ret);
    assert!(ret.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "history", CARGO_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
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
    assert!(execute_arg(&args).await.is_ok());

    // Try to stop a cargo
    let args = Cli::parse_from(["nanocl", "cargo", "stop", CARGO_NAME]);
    assert!(execute_arg(&args).await.is_ok());
    // Try to remove cargo
    let args = Cli::parse_from(["nanocl", "cargo", "rm", "-y", CARGO_NAME]);
    assert!(execute_arg(&args).await.is_ok());
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
    _ = execute_arg(&args).await;

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    // History
    let args =
      Cli::parse_from(["nanocl", "resource", "history", "deploy-example.com"]);
    assert!(execute_arg(&args).await.is_ok());
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let history = client
      .list_history_resource("deploy-example.com")
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();
    let args = Cli::parse_from([
      "nanocl",
      "resource",
      "revert",
      "deploy-example.com",
      &history.key.to_string(),
    ]);
    assert!(execute_arg(&args).await.is_ok());
    // Remove resource
    let args =
      Cli::parse_from(["nanocl", "resource", "rm", "-y", "deploy-example.com"]);
    assert!(execute_arg(&args).await.is_ok());
    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());
  }

  /// Test cargo exec command
  #[ntex::test]
  async fn cargo_exec() {
    let mut args = Cli::parse_from([
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
    assert!(execute_arg(&args).await.is_ok());

    args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "-e",
      "A=test",
      "--",
      "env",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "--privileged",
      "--",
      "whoami",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "-t",
      "--",
      "ls",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    args = Cli::parse_from([
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "-u",
      "0",
      "--",
      "whoami",
    ]);
    assert!(execute_arg(&args).await.is_ok());
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
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.toml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-pys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "logs",
      "-t",
      "10",
      "--timestamps",
      "-s",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/cargo_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from([
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    ]);
    assert!(execute_arg(&args).await.is_ok());
  }

  #[ntex::test]
  async fn info() {
    let args = Cli::parse_from(["nanocl", "info"]);
    assert!(execute_arg(&args).await.is_ok());
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
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "stop", "cli-test-run"]);
    assert!(execute_arg(&args).await.is_ok());

    let args = Cli::parse_from(["nanocl", "cargo", "rm", "-y", "cli-test-run"]);
    assert!(execute_arg(&args).await.is_ok());
  }

  #[ntex::test]
  async fn node_list() {
    let args = Cli::parse_from(["nanocl", "node", "ls"]);
    assert!(execute_arg(&args).await.is_ok());
  }
}
