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
  use std::{path::Path, env};

  use crate::utils::test::get_test_client;

  use super::*;

  /// Test version command
  #[ntex::test]
  async fn version() {
    assert_cli_ok!("version");
  }

  /// Test Namespace commands
  #[ntex::test]
  async fn namespace() {
    const NAMESPACE_NAME: &str = "cli-namespace";

    // Try to create namespace
    assert_cli_ok!("namespace", "create", NAMESPACE_NAME);
    // Try to list namespaces
    assert_cli_ok!("namespace", "ls");
    // Try to inspect namespace
    assert_cli_ok!("namespace", "inspect", NAMESPACE_NAME);
    // Try to remove namespace
    assert_cli_ok!("namespace", "rm", "-y", NAMESPACE_NAME);
  }

  /// Test Cargo image commands
  #[ntex::test]
  async fn cargo_image() {
    const IMAGE_NAME: &str = "busybox:1.26.0";
    // Try to create cargo image
    assert_cli_ok!("cargo", "image", "pull", IMAGE_NAME);
    // Try to list cargo images
    assert_cli_ok!("cargo", "image", "ls");
    // Try to inspect cargo image
    assert_cli_ok!("cargo", "image", "inspect", IMAGE_NAME);
    // Try to remove cargo image
    assert_cli_ok!("cargo", "image", "rm", "-y", IMAGE_NAME);

    assert_cli_ok!(
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
    const IMAGE_NAME: &str = "ghcr.io/nxthat/nanocl-get-started:latest";
    const NAMESPACE_NAME: Option<&str> = None;
    let client = get_test_client();
    // Try to create cargo
    assert_cli_ok!("cargo", "create", CARGO_NAME, IMAGE_NAME);
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "created");

    // Try to list cargoes
    assert_cli_ok!("cargo", "ls");
    // Try to start a cargo
    assert_cli_ok!("cargo", "start", CARGO_NAME);
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "running");
    // Try to inspect a cargo
    assert_cli_ok!("cargo", "inspect", CARGO_NAME);
    // Try to inspect cargo json
    assert_cli_ok!("cargo", "inspect", "--display", "toml", CARGO_NAME);
    // Try to inspect cargo toml
    assert_cli_ok!("cargo", "inspect", "--display", "json", CARGO_NAME);
    // Try to patch a cargo
    assert_cli_ok!(
      "cargo", "patch", CARGO_NAME, "--image", IMAGE_NAME, "--env", "TEST=1",
    );

    assert_cli_ok!("cargo", "history", CARGO_NAME);
    let client = get_test_client();
    let history = client
      .list_history_cargo(CARGO_NAME, None)
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();

    assert_cli_ok!("cargo", "revert", CARGO_NAME, &history.key.to_string());

    // Try to stop a cargo
    assert_cli_ok!("cargo", "stop", CARGO_NAME);
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "exited");
    // Try to remove cargo
    assert_cli_ok!("cargo", "rm", "-y", CARGO_NAME);
    assert_cargo_not_exists!(client, CARGO_NAME, NAMESPACE_NAME);
    // Try to run cargo
    assert_cli_ok!("cargo", "run", CARGO_NAME, IMAGE_NAME);
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "running");

    // Try to remove cargo
    assert_cli_ok!("cargo", "rm", "-yf", CARGO_NAME);
    assert_cargo_not_exists!(client, CARGO_NAME, NAMESPACE_NAME);
  }

  /// Test Resource commands
  #[ntex::test]
  async fn resource() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/basic_resources.yml",
    );

    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    );

    // History
    assert_cli_ok!("resource", "history", "deploy-example.com");
    let client = get_test_client();
    let history = client
      .list_history_resource("deploy-example.com")
      .await
      .unwrap()
      .first()
      .unwrap()
      .clone();
    assert_cli_ok!(
      "resource",
      "revert",
      "deploy-example.com",
      &history.key.to_string(),
    );
    // Remove resource
    assert_cli_ok!("resource", "rm", "-y", "deploy-example.com");
    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.yml");
  }

  /// Test cargo exec command
  #[ntex::test]
  async fn cargo_exec() {
    assert_cli_ok!(
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
  async fn state_apply_default_statefile_name() {
    let tests_path = Path::new("../../tests")
      .canonicalize()
      .expect("Can't cannonicalize tests folder path");
    env::set_current_dir(tests_path).expect("Can't move in tests folder");
    assert_cli_ok!("state", "apply", "-y");

    let tests_path = Path::new("./without_s_option")
      .canonicalize()
      .expect("Can't cannonicalize without_s_option folder path");
    env::set_current_dir(tests_path)
      .expect("Can't move in without_s_option folder");
    assert_cli_ok!("state", "apply", "-y");

    let tests_path = Path::new("../without_s_option_yml")
      .canonicalize()
      .expect("Can't cannonicalize without_s_option_yml folder path");
    env::set_current_dir(tests_path)
      .expect("Can't move in without_s_option_yml folder");
    assert_cli_ok!("state", "apply", "-y");

    let tests_path = Path::new("../../bin/nanocl")
      .canonicalize()
      .expect("Can't cannonicalize tests folder path");
    env::set_current_dir(tests_path).expect("Can't move back in nanocl folder");

    assert_cli_err!("state", "apply", "-y");
  }

  #[ntex::test]
  async fn state_apply_invalid_image() {
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../tests/invalid_init_container_image.yml",
    );
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../tests/invalid_container_image.yml",
    );
  }

  #[ntex::test]
  async fn state_apply_url_statefile() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "https://raw.githubusercontent.com/nxthat/nanocl/nightly/examples/deploy_example.yml",
    );

    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "https://raw.githubusercontent.com/nxthat/nanocl/nightly/examples/deploy_example.yml",
    );

    assert_cli_err!("state", "rm", "-ys", "https://google.com");

    assert_cli_err!(
      "state",
      "rm",
      "-ys",
      "https://feiwfioewjnoifjnewoifnoiwef.fr",
    );
  }

  #[ntex::test]
  async fn state_apply_binds() {
    assert_cli_ok!("state", "apply", "-ys", "../../tests/relative_bind.yml");
    assert!(
      Path::new("./toto")
        .canonicalize()
        .expect("Can't cannonicalize bind toto folder path")
        .exists(),
      "Relative bind was not created",
    );
    assert!(
      Path::new("/tmp/toto")
        .canonicalize()
        .expect("Can't cannonicalize bind /tmp/toto folder path")
        .exists(),
      "Relative bind was not created",
    );
    assert_cli_ok!("state", "rm", "-ys", "../../tests/relative_bind.yml");
  }

  #[ntex::test]
  async fn state_apply_invalid_statefile() {
    assert_cli_err!("state", "apply", "-ys", "../../tests/invalid_yaml.yaml");
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../examples/invalid_json.json",
    );

    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../examples/invalid_toml.toml",
    );
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../examples/invalid_statefile.yaml",
    );
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../examples/invalid_statefile.toml",
    );
  }

  #[ntex::test]
  async fn state_apply_toml() {
    let client = get_test_client();
    const DEPLOY_CARGO_NAME: &str = "deploy-example";
    const DEPLOY_CARGO2_NAME: &str = "deploy-example2";
    const DEPLOY_NAMESPACE_NAME: Option<&str> = None;

    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.toml",
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO2_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );

    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.toml");
    assert_cargo_not_exists!(client, DEPLOY_CARGO_NAME, DEPLOY_NAMESPACE_NAME);
    assert_cargo_not_exists!(client, DEPLOY_CARGO2_NAME, DEPLOY_NAMESPACE_NAME);
  }

  #[ntex::test]
  async fn state_apply_json() {
    let client = get_test_client();
    const DEPLOY_CARGO_NAME: &str = "deploy-example";
    const DEPLOY_CARGO2_NAME: &str = "deploy-example2";
    const DEPLOY_NAMESPACE_NAME: Option<&str> = None;

    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.json",
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO2_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );

    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.json");

    assert_cargo_not_exists!(client, DEPLOY_CARGO_NAME, DEPLOY_NAMESPACE_NAME);
    assert_cargo_not_exists!(client, DEPLOY_CARGO2_NAME, DEPLOY_NAMESPACE_NAME);
  }

  #[ntex::test]
  async fn state_logs_invalide_statefile_kind() {
    assert_cli_err!("state", "logs", "-s", "../../examples/secret_env.yml");
  }

  #[ntex::test]
  async fn state() {
    let client = get_test_client();
    const DEPLOY_CARGO_NAME: &str = "deploy-example";
    const DEPLOY_CARGO2_NAME: &str = "deploy-example2";
    const DEPLOY_NAMESPACE_NAME: Option<&str> = None;
    const CARGO_NAME: &str = "cargo-example";
    const CARGO_NAMESPACE_NAME: Option<&str> = Some("cargo-example");

    assert_cli_ok!(
      "state",
      "apply",
      "-pys",
      "../../examples/deploy_example.yml",
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO2_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );

    assert_cli_ok!(
      "state",
      "apply",
      "-rys",
      "../../examples/deploy_example.toml"
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );
    assert_cargo_state!(
      client,
      DEPLOY_CARGO2_NAME,
      DEPLOY_NAMESPACE_NAME,
      "running"
    );

    assert_cli_ok!(
      "state",
      "logs",
      "-t",
      "10",
      "--timestamps",
      "-s",
      "../../examples/deploy_example.yml",
    );

    assert_cli_ok!("state", "logs", "-s", "../../examples/deploy_example.yml");

    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.yml");
    assert_cargo_not_exists!(client, DEPLOY_CARGO_NAME, DEPLOY_NAMESPACE_NAME);
    assert_cargo_not_exists!(client, DEPLOY_CARGO2_NAME, DEPLOY_NAMESPACE_NAME);

    assert_cli_ok!("state", "apply", "-ys", "../../examples/cargo_example.yml");
    assert_cargo_state!(client, CARGO_NAME, CARGO_NAMESPACE_NAME, "running");

    assert_cli_ok!("state", "apply", "-ys", "../../examples/cargo_example.yml");
    assert_cargo_state!(client, CARGO_NAME, CARGO_NAMESPACE_NAME, "running");

    assert_cli_ok!("state", "rm", "-ys", "../../examples/cargo_example.yml");
    assert_cargo_not_exists!(client, CARGO_NAME, CARGO_NAMESPACE_NAME);
  }

  #[ntex::test]
  async fn info() {
    assert_cli_ok!("info");
  }

  #[ntex::test]
  async fn cargo_run() {
    let client = get_test_client();
    const CARGO_NAME: &str = "cli-test-run";
    const NAMESPACE_NAME: Option<&str> = None;

    assert_cli_ok!(
      "cargo",
      "run",
      "cli-test-run",
      "ghcr.io/nxthat/nanocl-get-started:latest",
      "-e",
      "MESSAGE=GREETING",
    );
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "running");

    assert_cli_ok!("cargo", "stop", "cli-test-run");
    assert_cargo_state!(client, CARGO_NAME, NAMESPACE_NAME, "exited");

    assert_cli_ok!("cargo", "rm", "-y", "cli-test-run");
    assert_cargo_not_exists!(client, CARGO_NAME, NAMESPACE_NAME);
  }

  #[ntex::test]
  async fn cargo_inspect_invalid() {
    assert_cli_err!("cargo", "inspect", "ewfwefew");
  }

  #[ntex::test]
  async fn cargo_logs() {
    assert_cli_ok!("cargo", "-n", "system", "logs", "nanocld");
  }

  // #[ntex::test]
  // async fn cargo_wait() {
  //   let args = Cli::parse_from([
  //     "nanocl",
  //     "state",
  //     "apply",
  //     "-ys",
  //     "../../examples/static_replication.yml",
  //   ]);
  //   assert!(execute_arg(&args).await.is_ok());
  //   let args =
  //     Cli::parse_from(["nanocl", "cargo", "wait", "static-replicated"]);
  //   assert!(execute_arg(&args).await.is_ok());

  //   let args = Cli::parse_from([
  //     "nanocl",
  //     "state",
  //     "rm",
  //     "-ys",
  //     "../../examples/static_replication.yml",
  //   ]);
  //   assert!(execute_arg(&args).await.is_ok());

  //   let args = Cli::parse_from([
  //     "nanocl",
  //     "cargo",
  //     "wait",
  //     "-c",
  //     "not-running",
  //     "static-replicated",
  //   ]);
  //   assert!(execute_arg(&args).await.is_ok());

  //   let args = Cli::parse_from([
  //     "nanocl",
  //     "state",
  //     "rm",
  //     "-ys",
  //     "../../examples/static_replication.yml",
  //   ]);
  //   assert!(execute_arg(&args).await.is_ok());
  // }

  #[ntex::test]
  async fn node_list() {
    assert_cli_ok!("node", "ls");
  }

  #[ntex::test]
  async fn http_logs() {
    assert_cli_ok!("system", "http", "logs");
  }
}
