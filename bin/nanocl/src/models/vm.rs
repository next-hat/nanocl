use clap::{Parser, Subcommand};

use super::VmImageArgs;

#[derive(Debug, Subcommand)]
pub enum VmCommands {
  /// Manage vm images
  Image(VmImageArgs),
}

/// Manage configuration states
#[derive(Debug, Parser)]
pub struct VmArgs {
  #[clap(subcommand)]
  pub commands: VmCommands,
}
