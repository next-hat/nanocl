use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct StateOpts {
  /// Path or url to the state file
  #[clap(long, short = 'f')]
  pub file_path: String,
  #[clap(long, short = 'a')]
  pub attach: bool,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {
  /// Apply a state from a configuration file
  Apply(StateOpts),
  /// Revert a state from a configuration file
  Revert(StateOpts),
}

/// Manage configuration states
#[derive(Debug, Parser)]
pub struct StateArgs {
  #[clap(subcommand)]
  pub commands: StateCommands,
}
