use clap::Parser;

/// Nanocl Daemon - Self Sufficient Orchestrator
#[derive(Debug, Clone, Parser)]
#[command(name = "Nanocl")]
#[command(author = "Next Hat team <team@next-hat.com>")]
#[command(version)]
pub struct Cli {
  /// Hosts to listen to use tcp:// and unix:// [default: unix:///run/nanocl.sock]
  #[clap(short = 'H', long = "hosts")]
  pub hosts: Option<Vec<String>>,
  /// Docker daemon socket to connect [default: unix:///var/run/docker.sock]
  #[clap(long)]
  pub docker_host: Option<String>,
  /// State directory
  /// [default: /var/lib/nanocl]
  #[clap(long)]
  pub state_dir: Option<String>,
  /// Config directory
  #[clap(long, default_value = "/etc/nanocl")]
  pub conf_dir: String,
  /// Gateway automatically detected to host default source ip gateway if not set
  #[clap(long)]
  pub gateway: Option<String>,
  /// Hostname to use for the node automatically detected if not set
  #[clap(long)]
  pub hostname: Option<String>,
  /// Join current node to a cluster
  #[clap(long = "node")]
  pub nodes: Vec<String>,
  /// Address to advertise to other nodes
  #[clap(long = "advertise-addr")]
  pub advertise_addr: Option<String>,
  /// Group id
  #[clap(long, default_value = "0")]
  pub gid: u32,
}

/// Cli arguments unit test
#[cfg(test)]
mod tests {
  use super::*;

  /// Test cli arguments with default values
  #[test]
  fn cli_with_default() {
    let args = Cli::parse_from(["nanocl"]);
    assert_eq!(args.hosts, None);
    assert_eq!(args.docker_host, None);
    assert_eq!(args.state_dir, None);
    assert_eq!(args.conf_dir, String::from("/etc/nanocl"));
  }

  /// Test cli arguments with custom values
  #[test]
  fn cli_with_custom() {
    let args = Cli::parse_from([
      "nanocl",
      "-H",
      "unix:///run/nanocl.sock",
      "--docker-host",
      "/var/run/docker.sock",
      "--state-dir",
      "/var/lib/nanocl",
      "--conf-dir",
      "/etc/nanocl",
    ]);
    assert_eq!(
      args.hosts,
      Some(vec![String::from("unix:///run/nanocl.sock")])
    );
    assert_eq!(args.docker_host, Some(String::from("/var/run/docker.sock")));
    assert_eq!(args.state_dir, Some(String::from("/var/lib/nanocl")));
    assert_eq!(args.conf_dir, String::from("/etc/nanocl"));
  }
}
