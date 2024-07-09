use std::{
  fmt::{Display, Formatter},
  path::PathBuf,
};

use clap::{Parser, Subcommand};

use super::DisplayFormat;

/// `nanocl state apply` available options
#[derive(Parser, Clone)]
pub struct StateApplyOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub state_location: Option<String>,
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
  /// Remove orphaned elements
  #[clap(long)]
  pub remove_orphans: bool,
}

/// `nanocl state logs` available options
#[derive(Default, Parser)]
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

/// `nanocl state rm` available options
#[derive(Parser)]
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

/// `nanocl state` available commands
#[derive(Subcommand)]
pub enum StateCommand {
  /// Create or Update elements from a Statefile
  Apply(StateApplyOpts),
  /// Logs elements from a Statefile
  Logs(StateLogsOpts),
  /// Remove elements from a Statefile
  #[clap(alias("rm"))]
  Remove(StateRemoveOpts),
}

/// `nanocl state` available arguments
#[derive(Parser)]
pub struct StateArg {
  #[clap(subcommand)]
  pub command: StateCommand,
}

#[derive(Clone, Default, Debug)]
pub enum StateRoot {
  #[default]
  None,
  Url(String),
  File(PathBuf),
}

impl Display for StateRoot {
  fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
    match self {
      StateRoot::File(path) => {
        write!(f, "{}", path.as_os_str().to_str().expect("can't get root"))
      }
      StateRoot::Url(url) => write!(f, "{}", url),
      StateRoot::None => write!(f, ""),
    }
  }
}

/// Reference to a Statefile with his metadata once serialized
#[derive(Clone)]
pub struct StateRef<T>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  /// Raw data of the Statefile
  pub raw: String,
  /// Format of the Statefile
  pub format: DisplayFormat,
  /// Data of the Statefile (serialized)
  pub data: T,
  /// Include directory of the Statefile
  pub root: StateRoot,
  /// Path to the Statefile
  pub location: String,
}
