use clap::Parser;

/// Nanocl daemon
/// Self Sufficient Hybrid Cloud Orchestrator
#[derive(Debug, Clone, Parser)]
#[command(name = "Nanocl")]
#[command(author = "nexthat team <team@next-hat.com>")]
#[command(version)]
pub struct Cli {
  /// Ensure state is inited
  #[clap(long)]
  pub(crate) init: bool,
  /// Hosts to listen to use tcp:// and unix:// [default: unix:///run/nanocl.sock]
  #[clap(short = 'H', long = "hosts")]
  pub(crate) hosts: Option<Vec<String>>,
  /// Docker daemon socket to connect [default: unix:///run/docker.sock]
  #[clap(long)]
  pub(crate) docker_host: Option<String>,
  /// State directory
  /// [default: /var/lib/nanocl]
  #[clap(long)]
  pub(crate) state_dir: Option<String>,
  /// Config directory
  #[clap(long, default_value = "/etc/nanocl")]
  pub(crate) config_dir: String,
  /// Host gateway automatically detected to host default gateway if not set
  pub(crate) host_gateway: Option<String>,
}

/// Cli arguments unit test
#[cfg(test)]
mod tests {
  use super::*;

  /// Test cli arguments with default values
  #[test]
  fn test_cli_with_default() {
    let args = Cli::parse_from(["nanocl"]);
    assert_eq!(args.hosts, None);
    assert!(!args.init);
    assert_eq!(args.docker_host, None);
    assert_eq!(args.state_dir, None);
    assert_eq!(args.config_dir, String::from("/etc/nanocl"));
  }

  /// Test cli arguments with custom values
  #[test]
  fn test_cli_with_custom() {
    let args = Cli::parse_from([
      "nanocl",
      "-H",
      "unix:///run/nanocl.sock",
      "--docker-host",
      "/run/docker.sock",
      "--state-dir",
      "/var/lib/nanocl",
      "--config-dir",
      "/etc/nanocl",
    ]);
    assert_eq!(
      args.hosts,
      Some(vec![String::from("unix:///run/nanocl.sock")])
    );
    assert!(!args.init);
    assert_eq!(args.docker_host, Some(String::from("/run/docker.sock")));
    assert_eq!(args.state_dir, Some(String::from("/var/lib/nanocl")));
    assert_eq!(args.config_dir, String::from("/etc/nanocl"));
  }
}
