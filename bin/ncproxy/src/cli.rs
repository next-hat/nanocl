use clap::Parser;

#[derive(Parser)]
pub struct Cli {
  /// Path to haproxy config directory
  #[clap(
    long = "haproxy-dir",
    alias = "nginx-dir",
    default_value = "/etc/haproxy"
  )]
  pub haproxy_dir: String,
  /// Path to state directory
  #[clap(long)]
  pub state_dir: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    let args = Cli::parse_from([
      "ncproxy",
      "--haproxy-dir",
      "/test/haproxy",
      "--state-dir",
      "/test/state",
    ]);
    assert_eq!(args.haproxy_dir, "/test/haproxy");
    assert_eq!(args.state_dir, "/test/state");
    let args = Cli::parse_from(["ncproxy", "--state-dir", "/test/state"]);
    assert_eq!(args.haproxy_dir, "/etc/haproxy");
    assert_eq!(args.state_dir, "/test/state");
    let _ = Cli::try_parse();
  }
}
