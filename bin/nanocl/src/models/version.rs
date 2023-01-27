use clap::{Parser, Subcommand};

/// Version commands
#[derive(Debug, Subcommand)]
pub enum VersionCommands {
  /// Check the latest version of nanocl
  Check,
}

/// Show nanocl version information
#[derive(Debug, Parser)]
#[clap(name = "nanocl-version")]
pub struct VersionArgs {
  #[clap(subcommand)]
  pub command: Option<VersionCommands>,
}
