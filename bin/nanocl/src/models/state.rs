use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StateBuildArg {
  pub name: String,
  pub r#type: String,
  pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct StateBuildArgs {
  pub args: Option<Vec<StateBuildArg>>,
}

#[derive(Debug, Parser)]
pub struct StateOpts {
  /// Path or url to the state
  #[clap(long, short = 'f')]
  pub file_path: String,
  /// Attach to the logs of the deployed cargo when applying a state
  #[clap(long, short = 'a')]
  pub attach: bool,
  /// Skip the confirmation prompt
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true)]
  pub args: Vec<String>,
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
