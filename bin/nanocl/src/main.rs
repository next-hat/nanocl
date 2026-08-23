use clap::Parser;
use dotenvy::dotenv;

use nanocl_error::io::{IoError, IoResult};
use nanocld_client::{ConnectOpts, NanocldClient, stubs::system::SslConfig};

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
      let cert =
        std::fs::read_to_string(ssl.cert.clone().expect("cert file unset"))?;
      let cert_key = std::fs::read_to_string(
        ssl.cert_key.clone().expect("cert key file unset"),
      )?;
      Some(SslConfig {
        cert: Some(cert),
        cert_key: Some(cert_key),
        ..Default::default()
      })
    }
    None => None,
  };
  if let Ok(cert) = std::env::var("CERT") {
    let cert_key = std::env::var("CERT_KEY").ok();
    let cert_ca = std::env::var("CERT_CA").ok();
    ssl = Some(SslConfig {
      cert: Some(cert),
      cert_key,
      cert_ca: cert_ca.clone(),
      verify: cert_ca.is_some(),
      ..Default::default()
    });
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
    Command::Stats(args) => commands::exec_stats(&cli_conf, args).await,
    Command::Version => commands::exec_version(&cli_conf).await,
    Command::Vm(args) => commands::exec_vm(&cli_conf, args).await,
    Command::Logs(args) => commands::logs_process(&cli_conf, args).await,
    Command::Inspect(args) => commands::inspect_process(&cli_conf, args).await,
    Command::Kill(args) => commands::kill_process(&cli_conf, args).await,
    Command::Ps(args) => commands::exec_process(&cli_conf, args).await,
    Command::Install(args) => {
      #[cfg(not(target_os = "windows"))]
      {
        commands::exec_install(args).await
      }
      #[cfg(target_os = "windows")]
      {
        println!("Install is not supported on windows yet");
        Ok(())
      }
    }
    Command::Uninstall(args) => {
      #[cfg(not(target_os = "windows"))]
      {
        commands::exec_uninstall(args).await
      }
      #[cfg(target_os = "windows")]
      {
        println!("Uninstall is not supported on windows yet");
        Ok(())
      }
    }
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
    const CARGO_KEY: &str = "global.cli-test";
    assert_cli_ok!("cargo", "start", CARGO_KEY);
    // Try to inspect a cargo
    assert_cli_ok!("cargo", "inspect", CARGO_KEY);
    // Try to inspect cargo json
    assert_cli_ok!("cargo", "inspect", "--display", "toml", CARGO_KEY);
    // Try to inspect cargo toml
    assert_cli_ok!("cargo", "inspect", "--display", "json", CARGO_KEY);
    // Try to patch a cargo
    assert_cli_ok!(
      "cargo", "patch", CARGO_KEY, "--image", IMAGE_NAME, "--env", "TEST=1",
    );
    assert_cli_ok!("cargo", "history", CARGO_KEY);
    let client = get_test_client();
    let history = client
      .list_history_cargo(CARGO_KEY)
      .await
      .unwrap()
      .last()
      .unwrap()
      .clone();
    assert_cli_ok!("cargo", "revert", CARGO_KEY, &history.key.to_string());
    // Try to stop a cargo
    assert_cli_ok!("cargo", "stop", CARGO_KEY);
    // Try to remove cargo
    assert_cli_ok!("cargo", "rm", "-fy", CARGO_KEY);
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

  #[test]
  fn cargo_exec_and_kill_are_not_cli_subcommands() {
    assert!(
      Cli::try_parse_from([
        "nanocl",
        "cargo",
        "exec",
        "system.nstore",
        "--",
        "echo",
        "hello",
      ])
      .is_err()
    );
    assert!(
      Cli::try_parse_from(["nanocl", "cargo", "kill", "system.nstore"])
        .is_err()
    );
  }

  #[test]
  fn kill_command_defaults_to_sigkill() {
    let cli = Cli::try_parse_from(["nanocl", "kill", "global.foo-r0-abc.c"])
      .expect("kill command must parse");
    let Command::Kill(opts) = cli.command else {
      panic!("expected kill command");
    };
    assert_eq!(opts.process, "global.foo-r0-abc.c");
    assert_eq!(opts.signal, "SIGKILL");
  }

  #[test]
  fn kill_command_accepts_short_signal() {
    let cli = Cli::try_parse_from([
      "nanocl",
      "kill",
      "-s",
      "SIGTERM",
      "global.foo-r0-abc.c",
    ])
    .expect("kill command with short signal must parse");
    let Command::Kill(opts) = cli.command else {
      panic!("expected kill command");
    };
    assert_eq!(opts.process, "global.foo-r0-abc.c");
    assert_eq!(opts.signal, "SIGTERM");
  }

  #[test]
  fn kill_command_accepts_long_signal() {
    let cli = Cli::try_parse_from([
      "nanocl",
      "kill",
      "--signal",
      "SIGINT",
      "9f8e7d6c5b4a",
    ])
    .expect("kill command with long signal must parse");
    let Command::Kill(opts) = cli.command else {
      panic!("expected kill command");
    };
    assert_eq!(opts.process, "9f8e7d6c5b4a");
    assert_eq!(opts.signal, "SIGINT");
  }

  #[test]
  fn kill_command_requires_process() {
    assert!(Cli::try_parse_from(["nanocl", "kill"]).is_err());
  }

  #[test]
  fn kill_command_rejects_multiple_processes() {
    assert!(
      Cli::try_parse_from(["nanocl", "kill", "process-one", "process-two"])
        .is_err()
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
  }

  #[ntex::test]
  async fn state_apply_args_advanced() {
    // Case 1: Apply with required args only (defaults kick in for others)
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

    // Case 2: Apply overriding defaults including multi-value args
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
      // boolean flag
      "--ssl_enabled",
      // override single default
      "--env",
      "prod",
      // override number default
      "--retries",
      "5",
      // override multi string defaults
      "--tags",
      "alpha",
      "--tags",
      "beta",
      // override multi number defaults
      "--extra_ports",
      "7000",
      "--extra_ports",
      "7001",
      // provide value for fields with array defaults but Multiple=false
      "--color",
      "green",
      "--scale",
      "4",
    );

    // Case 3: Remove using the same required identification
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

  /// Test state render command for different formats and with args
  #[ntex::test]
  async fn state_render() {
    // YAML output
    assert_cli_ok!(
      "state",
      "render",
      "-ys",
      "../../examples/deploy_example.yml",
      "-o",
      "/tmp/nanocl_tests/render_example.yml"
    );
    assert!(
      Path::new("/tmp/nanocl_tests/render_example.yml").exists(),
      "Rendered YAML file not found"
    );
    // TOML output
    assert_cli_ok!(
      "state",
      "render",
      "-ys",
      "../../examples/deploy_example.yml",
      "-o",
      "/tmp/nanocl_tests/render_example.toml"
    );
    assert!(
      Path::new("/tmp/nanocl_tests/render_example.toml").exists(),
      "Rendered TOML file not found"
    );
    // JSON output
    assert_cli_ok!(
      "state",
      "render",
      "-ys",
      "../../examples/deploy_example.yml",
      "-o",
      "/tmp/nanocl_tests/render_example.json"
    );
    assert!(
      Path::new("/tmp/nanocl_tests/render_example.json").exists(),
      "Rendered JSON file not found"
    );
    // No extension (defaults to YAML) and args usage
    assert_cli_ok!(
      "state",
      "render",
      "-ys",
      "../../examples/args_advanced.yml",
      "-o",
      "/tmp/nanocl_tests/render_args_output",
      "--",
      "--name",
      "test-args-advanced",
      "--domain",
      "test.args.advanced.com",
      "--image",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "--port",
      "9000"
    );
    assert!(
      Path::new("/tmp/nanocl_tests/render_args_output").exists(),
      "Rendered default YAML file (no extension) not found"
    );
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
    const CARGO_KEY: &str = "global.cli-test-run";
    assert_cli_ok!(
      "cargo",
      "run",
      "cli-test-run",
      "ghcr.io/next-hat/nanocl-get-started:latest",
      "-e",
      "MESSAGE=GREETING",
    );
    ntex::rt::spawn(async {
      assert_cli_ok!("cargo", "stats", CARGO_KEY);
    });
    assert_cli_ok!("cargo", "stop", CARGO_KEY);
    assert_cli_ok!("cargo", "ls");
    assert_cli_ok!("cargo", "ls", "-q");
    assert_cli_ok!("cargo", "rm", "-fy", CARGO_KEY);
  }

  #[ntex::test]
  async fn job_basic() {
    assert_cli_ok!("state", "apply", "-yfs", "../../examples/job_example.yml");
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
    assert_cli_ok!("cargo", "logs", "system.nanocld");
    assert_cli_ok!("cargo", "logs", "system.nstore");
    assert_cli_ok!("cargo", "logs", "system.nstore", "-t", "10");
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

  #[ntex::test]
  async fn secret() {
    assert_cli_ok!("secret", "ls");
    assert_cli_ok!("secret", "ls", "-q");
    assert_cli_err!("secret", "create", "test-cli", "tls");
    assert_cli_ok!(
      "secret",
      "create",
      "test-cli",
      "tls",
      "--certificate",
      "yoloh",
      "--certificate-key",
      "yoloh",
      "--certificate-client",
      "yoloh"
    );
    assert_cli_ok!("secret", "ls");
    assert_cli_ok!("secret", "ls", "-q");
    assert_cli_ok!("secret", "rm", "-y", "test-cli");
    assert_cli_ok!(
      "secret",
      "create",
      "test-cli",
      "tls",
      "--certificate-path",
      "../../tests/server.crt",
      "--certificate-key-path",
      "../../tests/server.key",
      "--certificate-client-path",
      "../../tests/ca.key"
    );
    assert_cli_ok!("secret", "inspect", "test-cli");
    assert_cli_ok!("secret", "rm", "-y", "test-cli");
  }

  /// Test Secret patch command for env secrets
  #[ntex::test]
  async fn secret_patch() {
    // Create an env secret
    assert_cli_ok!(
      "secret",
      "create",
      "env.test",
      "env",
      "FOO=bar",
      "HELLO=world"
    );
    // Patch the env secret with a new value for FOO
    assert_cli_ok!("secret", "patch", "env.test", "env", "FOO=baz");
    // Inspect the secret
    assert_cli_ok!("secret", "inspect", "env.test");
    // Cleanup
    assert_cli_ok!("secret", "rm", "-y", "env.test");
  }

  #[ntex::test]
  async fn virtual_machine() {
    const VM_KEY: &str = "global.test-cli-vm";
    assert_cli_ok!(
      "vm",
      "create",
      "test-cli-vm",
      "../../tests/ubuntu-24.04-minimal-cloudimg-amd64.img"
    );
    assert_cli_ok!("vm", "ls");
    assert_cli_ok!("vm", "inspect", VM_KEY);
    assert_cli_ok!("vm", "start", VM_KEY);
    assert_cli_ok!("vm", "stop", VM_KEY);
    assert_cli_ok!("vm", "rm", "-y", VM_KEY);
    assert_cli_ok!(
      "vm",
      "run",
      "test-cli-vm",
      "../../tests/ubuntu-24.04-minimal-cloudimg-amd64.img"
    );
    assert_cli_ok!("vm", "rm", "-y", VM_KEY);
  }

  #[ntex::test]
  async fn stats() {
    ntex::rt::spawn(async {
      assert_cli_ok!("stats", "system.nstore.c");
    });
  }

  #[ntex::test]
  async fn backup() {
    assert_cli_ok!("backup", "-yo", "../../tests/backup");
    assert_cli_ok!("backup", "-yo", "../../tests/backup");
  }

  #[ntex::test]
  async fn logs() {
    assert_cli_ok!("logs", "system.nstore.c");
    assert_cli_ok!("logs", "system.nstore.c", "-s", "0");
  }

  #[ntex::test]
  async fn inspect() {
    assert_cli_ok!("inspect", "system.nstore.c");
  }
}
