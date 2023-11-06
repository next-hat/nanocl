use clap::{Parser, Subcommand};
use nanocld_client::stubs::state::StateMeta;
use serde::{Serialize, Deserialize};

use super::DisplayFormat;

/// ## StateBuildArg
///
/// Statefile argument definition to build the Statefile
///
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BuildArg {
  /// Name of the build arg
  pub name: String,
  /// Kind of the build arg
  pub kind: String,
  /// Default value of the build arg
  pub default: Option<String>,
}

/// ## StateBuildArg
///
/// Statefile arguments definition to build the Statefile
///
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct StateBuildArg {
  pub args: Option<Vec<BuildArg>>,
}

/// ## StateApplyOpts
///
/// `nanocl state apply` available options
///
#[derive(Debug, Parser)]
pub struct StateApplyOpts {
  /// Path or Url to the Statefile
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
  /// Perform an apply even if state didn't changed
  #[clap(long, short = 'r')]
  pub reload: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

/// ## StateLogsOpts
///
/// `nanocl state logs` available options
///
#[derive(Debug, Parser)]
pub struct StateLogsOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub state_location: Option<String>,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
  /// Only include logs since unix timestamp
  #[clap(long)]
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  #[clap(short = 'u')]
  pub until: Option<i64>,
  /// If integer only return last n logs, if "all" returns all logs
  #[clap(short = 't')]
  pub tail: Option<String>,
  /// Bool, if set include timestamp to ever log line
  #[clap(long = "timestamps")]
  pub timestamps: bool,
  /// Bool, if set open the log as stream
  #[clap(short = 'f')]
  pub follow: bool,
}

/// ## StateRemoveOpts
///
/// `nanocl state rm` available options
///
#[derive(Debug, Parser)]
pub struct StateRemoveOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub state_location: Option<String>,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

/// ## StateCommand
///
/// `nanocl state` available commands
///
#[derive(Debug, Subcommand)]
pub enum StateCommand {
  /// Create or Update elements from a Statefile
  Apply(StateApplyOpts),
  /// Logs elements from a Statefile
  Logs(StateLogsOpts),
  /// Remove elements from a Statefile
  #[clap(alias("rm"))]
  Remove(StateRemoveOpts),
}

/// ## StateArg
///
/// `nanocl state` available arguments
///
#[derive(Debug, Parser)]
pub struct StateArg {
  #[clap(subcommand)]
  pub command: StateCommand,
}

/// ## StateRef
///
/// Reference to a Statefile with his metadata once serialized
///
#[derive(Clone, Debug)]
pub struct StateRef<T>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  /// Raw data of the Statefile
  pub raw: String,
  /// Format of the Statefile
  pub format: DisplayFormat,
  /// Metadata of the Statefile
  pub meta: StateMeta,
  /// Data of the Statefile (serialized)
  pub data: T,
}
