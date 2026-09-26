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
  pub source: Option<String>,
  /// Follow logs of the deployed cargo
  #[clap(long, short = 'f')]
  pub follow: bool,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Stream newline-delimited JSON to stdout (requires --yes)
  #[clap(long, requires = "skip_confirm", conflicts_with = "follow")]
  pub json: bool,
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

/// In-memory declarations observed or previewed in Statefile apply order.
pub type StateDiffSnapshot =
  std::collections::BTreeMap<(String, String), Option<serde_json::Value>>;

/// `nanocl state diff` available options
#[derive(Parser)]
pub struct StateDiffOpts {
  /// Path or URL to the Statefile
  #[clap(long, short = 's')]
  pub source: Option<String>,
  /// Output a single JSON document with secret values hidden
  #[clap(long)]
  pub json: bool,
  /// Keep the diff open in a pager (default in interactive terminals)
  #[clap(long, conflicts_with_all = ["no_pager", "json"])]
  pub pager: bool,
  /// Print directly to the terminal without a pager
  #[clap(long)]
  pub no_pager: bool,
  /// Preview the orphan removals performed by apply --remove-orphans
  #[clap(long)]
  pub remove_orphans: bool,
  /// Preview an apply with --reload
  #[clap(long, short = 'r')]
  pub reload: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

/// `nanocl state logs` available options
#[derive(Default, Parser)]
pub struct StateLogsOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub source: Option<String>,
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

/// `nanocl state render` available options
#[derive(Parser, Clone)]
pub struct StateRenderOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub source: Option<String>,
  /// Output path for the rendered statefile
  #[clap(long, short = 'o')]
  pub output: Option<String>,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

/// `nanocl state rm` available options
#[derive(Parser)]
pub struct StateRemoveOpts {
  /// Path or Url to the Statefile
  #[clap(long, short = 's')]
  pub source: Option<String>,
  /// Skip the confirmation prompt
  #[clap(long = "yes", short = 'y')]
  pub skip_confirm: bool,
  /// Stream newline-delimited JSON to stdout (requires --yes)
  #[clap(long, requires = "skip_confirm")]
  pub json: bool,
  /// Additional arguments to pass to the file
  #[clap(last = true, raw = true)]
  pub args: Vec<String>,
}

#[derive(Parser)]
pub struct StateManOpts {
  /// Path or URL to the statefile
  #[clap(long, short = 's')]
  pub source: String,
}

/// `nanocl state` available commands
#[derive(Subcommand)]
pub enum StateCommand {
  /// Display documentation for a statefile
  Man(StateManOpts),
  /// Create or Update elements from a Statefile
  Apply(StateApplyOpts),
  /// Preview Statefile changes without modifying the daemon
  Diff(StateDiffOpts),
  /// Render a Statefile with args to an output file
  Render(StateRenderOpts),
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

#[cfg(test)]
mod tests {
  use crate::models::{Cli, Command, StateCommand};
  use clap::Parser;

  #[test]
  fn state_diff_accepts_json_and_apply_preview_flags_without_confirmation() {
    let cli = Cli::try_parse_from([
      "nanocl",
      "state",
      "diff",
      "--json",
      "--remove-orphans",
      "--reload",
      "-s",
      "deploy.yml",
      "--",
      "--image",
      "alpine:latest",
    ])
    .unwrap();
    let Command::State(state) = cli.command else {
      panic!("expected state")
    };
    let StateCommand::Diff(opts) = state.command else {
      panic!("expected diff")
    };
    assert!(opts.json && opts.remove_orphans && opts.reload);
    assert!(!opts.pager && !opts.no_pager);
    assert_eq!(opts.source.as_deref(), Some("deploy.yml"));
    assert_eq!(opts.args, ["--image", "alpine:latest"]);
    assert!(Cli::try_parse_from(["nanocl", "state", "diff"]).is_ok());
  }

  #[test]
  fn state_diff_pager_flags_accept_modes_and_reject_conflicts() {
    for (flags, pager, no_pager, json) in [
      (vec![], false, false, false),
      (vec!["--pager"], true, false, false),
      (vec!["--no-pager"], false, true, false),
      (vec!["--json", "--no-pager"], false, true, true),
    ] {
      let cli = Cli::try_parse_from(
        ["nanocl", "state", "diff"].into_iter().chain(flags),
      )
      .unwrap();
      let Command::State(state) = cli.command else {
        panic!("expected state")
      };
      let StateCommand::Diff(opts) = state.command else {
        panic!("expected diff")
      };
      assert_eq!(
        (opts.pager, opts.no_pager, opts.json),
        (pager, no_pager, json)
      );
    }
    for conflict in ["--no-pager", "--json"] {
      let error =
        Cli::try_parse_from(["nanocl", "state", "diff", "--pager", conflict])
          .err()
          .expect("conflicting pager flags must fail");
      assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
  }

  #[test]
  fn state_json_flags_require_yes_and_reject_follow() {
    for command in ["apply", "remove", "rm"] {
      let cli =
        Cli::try_parse_from(["nanocl", "state", command, "--json", "-y"])
          .unwrap();
      let Command::State(state) = cli.command else {
        panic!("expected state command")
      };
      match state.command {
        StateCommand::Apply(opts) => assert!(opts.json && opts.skip_confirm),
        StateCommand::Remove(opts) => assert!(opts.json && opts.skip_confirm),
        _ => panic!("expected apply or remove"),
      }
      assert!(
        Cli::try_parse_from(["nanocl", "state", command, "--json"]).is_err()
      );
    }
    assert!(
      Cli::try_parse_from([
        "nanocl", "state", "apply", "--json", "-y", "--follow"
      ])
      .is_err()
    );
  }
}
