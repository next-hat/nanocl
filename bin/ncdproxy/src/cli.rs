use clap::Parser;

#[derive(Parser)]
pub(crate) struct Cli {
  /// Path to nginx config directory
  #[clap(long)]
  pub(crate) conf_dir: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    let args = Cli::parse_from(["nanocl-ncdproxy", "--conf-dir", "/etc/nginx"]);
    assert_eq!(args.conf_dir, Some("/etc/nginx".into()));
    let args = Cli::parse_from(["nanocl-ncdproxy"]);
    assert_eq!(args.conf_dir, None);
    let _ = Cli::try_parse();
  }
}
