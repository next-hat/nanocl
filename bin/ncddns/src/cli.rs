use clap::Parser;

/// Nanocl Controller Daemon DNS
#[derive(Debug, Parser)]
pub(crate) struct Cli {
  /// Path to the config directory
  #[clap(long)]
  pub(crate) conf_dir: Option<String>,
  /// Dns server address to resolve domain name if not existing in local
  #[clap(long)]
  pub(crate) dns: Vec<String>,
  /// Server address to listen on (default: unix:///run/nanocl/dns.sock)
  #[clap(long, default_value = "unix:///run/nanocl/dns.sock")]
  pub(crate) host: String,
}
