use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct StateBuildArg {
  pub name: String,
  pub kind: String,
  pub default: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct StateBuildArgs {
  pub args: Option<Vec<StateBuildArg>>,
}

#[derive(Debug, Parser)]
pub struct StateApplyOpts {
  /// Path or Url to the StateFile
  #[clap(long, short = 's')]
  pub state_location: Option<String>,
  /// Force pulling images even if they exist
  #[clap(long, short = 'p')]
  pub force_pull: bool,
  /// Follow logs of the deployed cargo
  #[clap(long, short = 'f')]
  pub follow: bool,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct StateRemoveOpts {
  /// Path or Url to the StateFile
  #[clap(long, short = 's')]
  pub state_location: Option<String>,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {
  /// Create or Update elements from a StateFile
  Apply(StateApplyOpts),
  /// Remove elements from a StateFile
  #[clap(alias("rm"))]
  Remove(StateRemoveOpts),
}

/// Define, Run, or Remove Cargo or Virtual Machines
#[derive(Debug, Parser)]
pub struct StateArgs {
  #[clap(subcommand)]
  pub commands: StateCommands,
}
