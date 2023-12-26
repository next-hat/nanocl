use clap::Parser;

#[derive(Parser)]
pub struct Cli {
  /// Path to nginx config directory
  #[clap(long, default_value = "/etc/nginx")]
  pub nginx_dir: String,
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
}
