use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct VmImageCreateBaseOpts {
  pub name: String,
  /// Path or url to the state
  pub file_path: String,
}

#[derive(Debug, Subcommand)]
pub enum VmImageCommands {
  /// Create a base VM image
  CreateBase(VmImageCreateBaseOpts),
}

/// Manage configuration states
#[derive(Debug, Parser)]
pub struct VmImageArgs {
  #[clap(subcommand)]
  pub commands: VmImageCommands,
}
