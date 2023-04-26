use clap::Parser;

/// Nanocl controller dns
#[derive(Debug, Parser)]
pub(crate) struct Cli {
  /// Path to the config directory
  #[clap(long)]
  pub(crate) conf_dir: Option<String>,
  /// Dns server address to resolve domain name if not existing in local
  #[clap(long)]
  pub(crate) dns: Vec<String>,
}
