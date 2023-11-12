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
  let client = NanocldClient::connect_to(&host, None);
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
    assert_cli_ok!("nanocl", "version");
  }

  /// Test Namespace commands
  #[ntex::test]
  async fn namespace() {
    const NAMESPACE_NAME: &str = "cli-namespace";
    // Try to create namespace
    assert_cli_ok!("nanocl", "namespace", "create", NAMESPACE_NAME);
    // Try to list namespaces
    assert_cli_ok!("nanocl", "namespace", "ls");
    // Try to inspect namespace
    assert_cli_ok!("nanocl", "namespace", "inspect", NAMESPACE_NAME);
    // Try to remove namespace
    assert_cli_ok!("nanocl", "namespace", "rm", "-y", NAMESPACE_NAME);
  }

  /// Test Cargo image commands
  #[ntex::test]
  async fn cargo_image() {
    const IMAGE_NAME: &str = "busybox:1.26.0";
    // Try to create cargo image
    assert_cli_ok!("nanocl", "cargo", "image", "pull", IMAGE_NAME);
    // Try to list cargo images
    assert_cli_ok!("nanocl", "cargo", "image", "ls");
    // Try to inspect cargo image
    assert_cli_ok!("nanocl", "cargo", "image", "inspect", IMAGE_NAME);
    // Try to remove cargo image
    assert_cli_ok!("nanocl", "cargo", "image", "rm", "-y", IMAGE_NAME);

    assert_cli_ok!(
      "nanocl",
      "cargo",
      "image",
      "import",
      "-f",
      "../../tests/busybox.tar.gz",
    );
  }

  /// Test Cargo commands
  #[ntex::test]
  async fn cargo() {
    const CARGO_NAME: &str = "cli-test";
    const IMAGE_NAME: &str = "ghcr.io/nxthat/nanocl-get-started:latest:latest";
    // Try to create cargo
    assert_cli_ok!("nanocl", "cargo", "create", CARGO_NAME, IMAGE_NAME);
    // Try to list cargoes
    assert_cli_ok!("nanocl", "cargo", "ls");
    // Try to start a cargo
    assert_cli_ok!("nanocl", "cargo", "start", CARGO_NAME);
    // Try to inspect a cargo
    assert_cli_ok!("nanocl", "cargo", "inspect", CARGO_NAME);
    // Try to patch a cargo
    assert_cli_ok!(
      "nanocl", "cargo", "patch", CARGO_NAME, "--image", IMAGE_NAME, "--env",
      "TEST=1",
    );

    assert_cli_ok!("nanocl", "cargo", "history", CARGO_NAME);
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let history = client
      .list_history_cargo(CARGO_NAME, None)
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();

    assert_cli_ok!(
      "nanocl",
      "cargo",
      "revert",
      CARGO_NAME,
      &history.key.to_string(),
    );

    // Try to stop a cargo
    assert_cli_ok!("nanocl", "cargo", "stop", CARGO_NAME);
    // Try to remove cargo
    assert_cli_ok!("nanocl", "cargo", "rm", "-y", CARGO_NAME);
  }

  /// Test Resource commands
  #[ntex::test]
  async fn resource() {
    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/basic_resources.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    );

    // History
    assert_cli_ok!("nanocl", "resource", "history", "deploy-example.com");
    let client =
      NanocldClient::connect_to("http://ndaemon.nanocl.internal:8585", None);
    let history = client
      .list_history_resource("deploy-example.com")
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();
    assert_cli_ok!(
      "nanocl",
      "resource",
      "revert",
      "deploy-example.com",
      &history.key.to_string(),
    );
    // Remove resource
    assert_cli_ok!("nanocl", "resource", "rm", "-y", "deploy-example.com");
    assert_cli_ok!(
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    );
  }

  /// Test cargo exec command
  #[ntex::test]
  async fn cargo_exec() {
    assert_cli_ok!(
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "--",
      "echo",
      "hello",
    );

    assert_cli_ok!(
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
    );

    assert_cli_ok!(
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "--privileged",
      "--",
      "whoami",
    );

    assert_cli_ok!(
      "nanocl",
      "cargo",
      "--namespace",
      "system",
      "exec",
      "nstore",
      "-t",
      "--",
      "ls",
    );

    assert_cli_ok!(
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
    );
  }

  #[ntex::test]
  async fn state() {
    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.toml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-rys",
      "../../examples/deploy_example.toml"
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-pys",
      "../../examples/deploy_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "logs",
      "-t",
      "10",
      "--timestamps",
      "-s",
      "../../examples/deploy_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "apply",
      "-ys",
      "../../examples/cargo_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/cargo_example.yml",
    );

    assert_cli_ok!(
      "nanocl",
      "state",
      "rm",
      "-ys",
      "../../examples/deploy_example.yml",
    );
  }

  #[ntex::test]
  async fn info() {
    assert_cli_ok!("nanocl", "info");
  }

  #[ntex::test]
  async fn cargo_run() {
    assert_cli_ok!(
      "nanocl",
      "cargo",
      "run",
      "cli-test-run",
      "ghcr.io/nxthat/nanocl-get-started:latest",
      "-e",
      "MESSAGE=GREETING",
    );

    assert_cli_ok!("nanocl", "cargo", "stop", "cli-test-run");
    assert_cli_ok!("nanocl", "cargo", "rm", "-y", "cli-test-run");
  }

  #[ntex::test]
  async fn node_list() {
    assert_cli_ok!("nanocl", "node", "ls");
  }
}
