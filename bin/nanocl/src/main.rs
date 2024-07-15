use clap::Parser;
use dotenvy::dotenv;

use nanocl_error::io::{IoError, IoResult};
use nanocld_client::{stubs::system::SslConfig, ConnectOpts, NanocldClient};

mod commands;
mod config;
mod models;
mod utils;
mod version;

use config::{CliConfig, UserConfig};
use models::{Cli, Command, Context};

/// Create a CliConfig struct from the cli arguments
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
  let endpoint = context.endpoints.get("Nanocl").unwrap();
  let mut host = cli_args.host.clone().unwrap_or(endpoint.host.clone());
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    if context.name == "default" {
      host = cli_args
        .host
        .clone()
        .unwrap_or("http://nanocl.internal:8585".into());
    }
  }
  let mut ssl = match &endpoint.ssl {
    Some(ssl) => {
      let cert = std::fs::read_to_string(ssl.cert.clone().unwrap())?;
      let cert_key = std::fs::read_to_string(ssl.cert_key.clone().unwrap())?;
      Some(SslConfig {
        cert: Some(cert),
        cert_key: Some(cert_key),
        ..Default::default()
      })
    }
    None => None,
  };
  if let Ok(c) = std::env::var("CERT") {
    if let Ok(ck) = std::env::var("CERT_KEY") {
      ssl = Some(SslConfig {
        cert: Some(c),
        cert_key: Some(ck),
        ..Default::default()
      });
    }
  }
  if let Ok(h) = std::env::var("HOST") {
    host = h;
  }
  let client = NanocldClient::connect_to(&ConnectOpts {
    url: host.clone(),
    ssl,
    ..Default::default()
  })?;
  Ok(CliConfig {
    host,
    client,
    context,
    user_config: user_conf,
  })
}

/// Execute the command from the cli arguments
async fn execute_arg(cli_args: &Cli) -> IoResult<()> {
  let cli_conf = create_cli_config(cli_args)?;
  match &cli_args.command {
    Command::Namespace(args) => commands::exec_namespace(&cli_conf, args).await,
    Command::Job(args) => commands::exec_job(&cli_conf, args).await,
    Command::Resource(args) => commands::exec_resource(&cli_conf, args).await,
    Command::Cargo(args) => commands::exec_cargo(&cli_conf, args).await,
    Command::Secret(args) => commands::exec_secret(&cli_conf, args).await,
    Command::Event(args) => commands::exec_event(&cli_conf, args).await,
    Command::State(args) => commands::exec_state(&cli_conf, args).await,
    Command::Version => commands::exec_version(&cli_conf).await,
    Command::Vm(args) => commands::exec_vm(&cli_conf, args).await,
    Command::Ps(args) => commands::exec_process(&cli_conf, args).await,
    Command::Install(args) => commands::exec_install(args).await,
    Command::Uninstall(args) => commands::exec_uninstall(args).await,
    Command::Node(args) => commands::exec_node(&cli_conf, args).await,
    Command::Context(args) => commands::exec_context(&cli_conf, args).await,
    Command::Info => commands::exec_info(&cli_conf).await,
    Command::Metric(args) => commands::exec_metric(&cli_conf, args).await,
    Command::Backup(opts) => commands::exec_backup(&cli_conf, opts).await,
  }
}

/// Nanocl is a command line interface for the Nanocl Daemon.
/// It will translate the corresponding commands to the Nanocl Daemon API.
/// You can use it to manage your cargoes and virtual machines.
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
  .map_err(|err| {
    IoError::interrupted("Signal", &format!("Unable to register ctrl-c: {err}"))
  })?;
  if let Err(err) = execute_arg(&args).await {
    err.print_and_exit();
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{env, path::Path};

  use crate::utils::tests::*;

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

  /// Test Cargo commands
  #[ntex::test]
  async fn cargo() {
    const CARGO_NAME: &str = "cli-test";
    const IMAGE_NAME: &str = "ghcr.io/next-hat/nanocl-get-started:latest";
    // Try to create cargo
    assert_cli_ok!("cargo", "create", CARGO_NAME, IMAGE_NAME);
    // Try to list cargoes
    assert_cli_ok!("cargo", "ls");
    // Try to start a cargo
    assert_cli_ok!("cargo", "start", CARGO_NAME);
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
      .last()
      .unwrap()
      .clone();
    assert_cli_ok!("cargo", "revert", CARGO_NAME, &history.key.to_string());
    // Try to stop a cargo
    assert_cli_ok!("cargo", "stop", CARGO_NAME);
    // Try to remove cargo
    assert_cli_ok!("cargo", "rm", "-fy", CARGO_NAME);
  }

  /// Test state file when then include other state files
  #[ntex::test]
  async fn sub_state() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000"
    );
    assert_cli_ok!(
      "state",
      "logs",
      "-s",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000"
    );
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000"
    );
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000",
      "--enable_job",
    );
    assert_cli_ok!(
      "state",
      "logs",
      "-s",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000",
      "--enable_job",
    );
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "../../examples/sub_state.yml",
      "--",
      "--name",
      "cli-test",
      "--port",
      "9000",
      "--enable_job",
    );
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../tests/invalid_sub_state.yml",
    );
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "https://nhnr.io/v0.14/tests/sub_state.yml",
    );
    assert_cli_ok!(
      "state",
      "logs",
      "-s",
      "https://nhnr.io/v0.14/tests/sub_state.yml"
    );
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "https://nhnr.io/v0.14/tests/sub_state.yml",
    );
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "nhnr.io/v0.14/tests/sub_state.yml",
    );
    assert_cli_ok!("state", "logs", "-s", "nhnr.io/v0.14/tests/sub_state.yml");
    assert_cli_ok!("state", "rm", "-ys", "nhnr.io/v0.14/tests/sub_state.yml",);
  }

  /// Test Resource commands
  #[ntex::test]
  async fn resource() {
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
      &history.key.to_string()
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
      .expect("Can't canonicalize tests folder path");
    env::set_current_dir(tests_path).expect("Can't move in tests folder");
    assert_cli_ok!("state", "apply", "-y");
    let tests_path = Path::new("./without_s_option")
      .canonicalize()
      .expect("Can't canonicalize without_s_option folder path");
    env::set_current_dir(tests_path)
      .expect("Can't move in without_s_option folder");
    assert_cli_ok!("state", "apply", "-y");
    let tests_path = Path::new("../without_s_option_yml")
      .canonicalize()
      .expect("Can't canonicalize without_s_option_yml folder path");
    env::set_current_dir(tests_path)
      .expect("Can't move in without_s_option_yml folder");
    assert_cli_ok!("state", "apply", "-y");
    let tests_path = Path::new("../../bin/nanocl")
      .canonicalize()
      .expect("Can't canonicalize tests folder path");
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
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "../../tests/invalid_init_container_image.yml",
    );
    assert_cli_err!(
      "state",
      "apply",
      "-ys",
      "../../tests/invalid_container_image.yml",
    );
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "../../tests/invalid_container_image.yml",
    );
  }

  #[ntex::test]
  async fn state_apply_remote_http() {
    assert_cli_ok!("state", "apply", "-ys", "../../tests/remote_http.yml");
    assert_cli_ok!("state", "apply", "-ys", "../../tests/remote_http.yml");
    assert_cli_ok!("state", "rm", "-ys", "../../tests/remote_http.yml");
  }

  #[ntex::test]
  async fn state_apply_include() {
    let filename = "include_example.yml";
    let relative_path = format!("../../examples/{filename}");
    assert_cli_ok!("state", "apply", "-ys", &relative_path);
    assert_cli_ok!("state", "rm", "-ys", &relative_path);
    let path = env::current_dir().unwrap();
    let path = path.as_os_str().to_str().unwrap();
    let absolute_path = format!("{path}/{relative_path}");
    assert_cli_ok!("state", "apply", "-ys", &absolute_path);
    assert_cli_ok!("state", "rm", "-ys", &absolute_path);
    let short_url = format!("nhnr.io/v0.14/tests/{filename}");
    assert_cli_ok!("state", "apply", "-ys", &short_url);
    assert_cli_ok!("state", "rm", "-ys", &short_url);
    let url = format!("https://nhnr.io/v0.14/tests/{filename}");
    assert_cli_ok!("state", "apply", "-ys", &url);
    assert_cli_ok!("state", "rm", "-ys", &url);
  }

  #[ntex::test]
  async fn state_apply_args_advanced() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/args_advanced.yml",
      "--",
      "--name",
      "test-args-advanced",
      "--domain",
      "test.args.advanced.com",
      "--image",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "--port",
      "9000",
    );
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/args_advanced.yml",
      "--",
      "--name",
      "test-args-advanced",
      "--domain",
      "test.args.advanced.com",
      "--image",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "--port",
      "9000",
    );
    assert_cli_ok!(
      "state",
      "rm",
      "-ys",
      "../../examples/args_advanced.yml",
      "--",
      "--name",
      "test-args-advanced",
      "--domain",
      "test.args.advanced.com",
      "--image",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "--port",
      "9000",
    );
  }

  #[ntex::test]
  async fn state_apply_url_statefile() {
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
      Path::new("/tmp/toto")
        .canonicalize()
        .expect("Can't canonicalize bind /tmp/toto folder path")
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
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.toml",
    );
    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.toml");
  }

  #[ntex::test]
  async fn state_apply_json() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.json",
    );
    assert_cli_ok!("state", "rm", "-ys", "../../examples/deploy_example.json");
  }

  #[ntex::test]
  async fn state() {
    assert_cli_ok!(
      "state",
      "apply",
      "-ys",
      "../../examples/deploy_example.yml",
    );
    assert_cli_ok!(
      "state",
      "apply",
      "-rys",
      "../../examples/deploy_example.toml"
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
    assert_cli_ok!("state", "apply", "-ys", "../../examples/cargo_example.yml");
    assert_cli_ok!("state", "apply", "-ys", "../../examples/cargo_example.yml");
    assert_cli_ok!("state", "rm", "-ys", "../../examples/cargo_example.yml");
  }

  #[ntex::test]
  async fn info() {
    assert_cli_ok!("info");
  }

  #[ntex::test]
  async fn cargo_basic() {
    const CARGO_NAME: &str = "cli-test-run";
    assert_cli_ok!(
      "cargo",
      "run",
      "cli-test-run",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "-e",
      "MESSAGE=GREETING",
    );
    ntex::rt::spawn(async {
      assert_cli_ok!("cargo", "stats", CARGO_NAME);
    });
    assert_cli_ok!("cargo", "stop", CARGO_NAME);
    assert_cli_ok!("cargo", "ls");
    assert_cli_ok!("cargo", "ls", "-q");
    assert_cli_ok!("cargo", "rm", "-fy", CARGO_NAME);
  }

  #[ntex::test]
  async fn job_basic() {
    assert_cli_ok!("state", "apply", "-ys", "../../examples/job_example.yml");
    assert_cli_ok!("job", "ls");
    assert_cli_ok!("job", "ls", "-q");
    assert_cli_ok!("job", "inspect", "job-example");
    assert_cli_ok!("job", "inspect", "job-example", "--display", "toml");
    assert_cli_ok!("job", "inspect", "job-example", "--display", "json");
    assert_cli_ok!("job", "logs", "job-example");
    assert_cli_ok!("job", "rm", "-y", "job-example");
    assert_cli_ok!("state", "rm", "-ys", "../../examples/job_example.yml");
  }

  #[ntex::test]
  async fn cargo_inspect_invalid() {
    assert_cli_err!("cargo", "inspect", "unknown-cargo");
  }

  #[ntex::test]
  async fn cargo_logs() {
    assert_cli_ok!("cargo", "-n", "system", "logs", "nanocld");
    assert_cli_ok!("cargo", "-n", "system", "logs", "nstore");
    assert_cli_ok!("cargo", "-n", "system", "logs", "nstore", "-t", "10");
  }

  #[ntex::test]
  async fn node_list() {
    assert_cli_ok!("node", "ls");
  }

  #[ntex::test]
  async fn ps() {
    assert_cli_ok!("ps");
    assert_cli_ok!("ps", "-n", "system");
    assert_cli_ok!("ps", "-a");
    assert_cli_ok!("ps", "--limit", "2");
    assert_cli_ok!("ps", "--offset", "1");
    assert_cli_ok!(
      "ps",
      "--namespace",
      "system",
      "--limit",
      "2",
      "--offset",
      "1"
    );
    assert_cli_ok!(
      "ps",
      "--namespace",
      "system",
      "--kind",
      "cargo",
      "--limit",
      "2",
      "--offset",
      "1"
    );
  }

  #[ntex::test]
  async fn metric() {
    assert_cli_ok!("metric", "ls");
    assert_cli_ok!("metric", "ls", "-q");
    assert_cli_ok!("metric", "ls", "--limit", "2");
    assert_cli_ok!("metric", "ls", "--offset", "1");
    assert_cli_ok!("metric", "ls", "--limit", "2", "--offset", "1");
    assert_cli_ok!("metric", "ls", "-q", "--limit", "2", "--offset", "1");
  }

  #[ntex::test]
  async fn event() {
    assert_cli_ok!("event", "ls");
    assert_cli_ok!("event", "ls", "-q");
    assert_cli_ok!("event", "ls", "--limit", "2");
    assert_cli_ok!("event", "ls", "--offset", "1");
    assert_cli_ok!("event", "ls", "--limit", "2", "--offset", "1");
    assert_cli_ok!("event", "ls", "-q", "--limit", "2", "--offset", "1");
  }
}
