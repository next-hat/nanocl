use clap::{Parser, Subcommand};

/// Resource commands
#[derive(Debug, Subcommand)]
pub enum VersionCommands {
  /// Check the latest version of nanocl
  Check,
}

/// Manage resources
#[derive(Debug, Parser)]
#[clap(name = "nanocl-version")]
pub struct VersionArgs {
  #[clap(subcommand)]
  pub command: Option<VersionCommands>,
}
