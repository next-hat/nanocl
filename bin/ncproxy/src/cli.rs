use clap::Parser;

#[derive(Parser)]
pub struct Cli {
  /// Path to nginx config directory
  #[clap(long, default_value = "/etc/nginx")]
  pub nginx_dir: String,
  /// Path to state directory
  #[clap(long)]
  pub state_dir: String,
  /// Recover startup fences after stopping every old controller and nginx.
  #[clap(long, requires = "acknowledge_nginx_stopped")]
  pub recover_nginx_fences: bool,
  /// Confirm all controllers, nginx masters, workers, and launchers sharing
  /// this state directory or /run are stopped and prevented from restarting.
  #[clap(long, requires = "recover_nginx_fences")]
  pub acknowledge_nginx_stopped: bool,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    let args = Cli::parse_from([
      "ncproxy",
      "--nginx-dir",
      "/test/nginx",
      "--state-dir",
      "/test/state",
    ]);
    assert_eq!(args.nginx_dir, "/test/nginx");
    assert_eq!(args.state_dir, "/test/state");
    let args = Cli::parse_from(["ncproxy", "--state-dir", "/test/state"]);
    assert_eq!(args.nginx_dir, "/etc/nginx");
    assert_eq!(args.state_dir, "/test/state");
    let _ = Cli::try_parse();
  }

  #[test]
  fn recovery_requires_explicit_stopped_acknowledgement() {
    assert!(
      Cli::try_parse_from([
        "ncproxy",
        "--state-dir",
        "/test/state",
        "--recover-nginx-fences"
      ])
      .is_err()
    );
    assert!(
      Cli::try_parse_from([
        "ncproxy",
        "--state-dir",
        "/test/state",
        "--acknowledge-nginx-stopped"
      ])
      .is_err()
    );
    let cli = Cli::try_parse_from([
      "ncproxy",
      "--state-dir",
      "/test/state",
      "--recover-nginx-fences",
      "--acknowledge-nginx-stopped",
    ])
    .unwrap();
    assert!(cli.recover_nginx_fences && cli.acknowledge_nginx_stopped);
  }
}
