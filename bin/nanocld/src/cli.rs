use clap::Parser;

/// Nanocl Daemon - Self Sufficient Hybrid Cloud Orchestrator
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
  pub(crate) conf_dir: String,
  /// Gateway automatically detected to host default source ip gateway if not set
  #[clap(long)]
  pub(crate) gateway: Option<String>,
  /// Hostname to use for the node automatically detected if not set
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  /// Join current node to a cluster
  #[clap(long = "node")]
  pub(crate) nodes: Vec<String>,
  /// Address to advertise to other nodes
  #[clap(long = "advertise-addr")]
  pub(crate) advertise_addr: Option<String>,
  /// Group id
  #[clap(long, default_value = "0")]
  pub(crate) gid: u32,
  /// SSL certificate
  #[clap(long)]
  pub(crate) ssl_cert: Option<String>,
  /// SSL key
  #[clap(long)]
  pub(crate) ssl_key: Option<String>,
  /// SSL CA
  #[clap(long)]
  pub(crate) ssl_ca: Option<String>,
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
    assert!(!args.init);
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
      "/run/docker.sock",
      "--state-dir",
      "/var/lib/nanocl",
      "--conf-dir",
      "/etc/nanocl",
    ]);
    assert_eq!(
      args.hosts,
      Some(vec![String::from("unix:///run/nanocl.sock")])
    );
    assert!(!args.init);
    assert_eq!(args.docker_host, Some(String::from("/run/docker.sock")));
    assert_eq!(args.state_dir, Some(String::from("/var/lib/nanocl")));
    assert_eq!(args.conf_dir, String::from("/etc/nanocl"));
  }
}
