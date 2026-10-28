use clap::{Args, Parser};
use serde::Deserialize;

use nanocld_client::stubs::{generic::GenericFilter, system::ObjPsStatus};

use super::DisplayFormat;

/// An empty filter to use as default
#[derive(Clone, Debug, Default, Args)]
pub struct GenericDefaultOpts;

/// A generic filter to use in the list operations
impl From<GenericDefaultOpts> for GenericFilter {
  fn from(_: GenericDefaultOpts) -> Self {
    Self::default()
  }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GenericProcessStatus {
  /// Status of the cargo
  pub status: ObjPsStatus,
}

/// Generic list options for the list command
#[derive(Default, Debug, Clone, Parser)]
pub struct GenericListOpts<T = GenericDefaultOpts>
where
  T: Args + Clone + Default,
{
  /// Only show keys
  #[clap(long, short)]
  pub quiet: bool,
  /// Show all columns and exact creation timestamps
  #[clap(long)]
  pub wide: bool,
  /// Limit the number of results default to 100
  #[clap(long, short)]
  pub limit: Option<usize>,
  /// Offset the results to navigate through the results
  #[clap(long, short)]
  pub offset: Option<usize>,
  /// Filters
  #[clap(long)]
  pub filters: Option<Vec<String>>,
  #[clap(flatten)]
  pub others: Option<T>,
}

/// Format a creation timestamp using the compact units shared by list commands.
pub(super) fn format_age<Tz: chrono::TimeZone>(
  created_at: Option<&chrono::DateTime<Tz>>,
  now: chrono::DateTime<chrono::Utc>,
) -> String {
  let Some(created_at) = created_at else {
    return "<unknown>".to_owned();
  };
  let seconds = now
    .signed_duration_since(created_at.clone())
    .num_seconds()
    .max(0);
  match seconds {
    0..60 => format!("{seconds}s"),
    60..3600 => format!("{}m", seconds / 60),
    3600..86400 => format!("{}h", seconds / 3600),
    _ => format!("{}d", seconds / 86400),
  }
}

/// List options for namespaced resource collections.
#[derive(Default, Debug, Clone, Parser)]
pub struct NamespacedListOpts {
  /// Optional namespace filter; omitted returns resources from all namespaces
  #[clap(long, short)]
  pub namespace: Option<String>,
  #[clap(flatten)]
  pub list: GenericListOpts,
}

/// Convert the generic list options to a generic filter
impl<T> From<GenericListOpts<T>> for GenericFilter
where
  T: Args + Clone + Default,
{
  fn from(opts: GenericListOpts<T>) -> Self {
    Self {
      limit: opts.limit,
      offset: opts.offset,
      ..Default::default()
    }
  }
}

/// Generic remove options for the remove command
#[derive(Clone, Parser)]
pub struct GenericRemoveOpts<T = GenericDefaultOpts>
where
  T: Args + Clone,
{
  /// The keys or names of the objects to remove
  pub keys: Vec<String>,
  #[clap(short = 'y', long)]
  pub skip_confirm: bool,
  /// Filters
  #[clap(flatten)]
  pub others: T,
}

/// Generic force options for the remove command
#[derive(Clone, Parser)]
pub struct GenericRemoveForceOpts {
  #[clap(short = 'f', long)]
  pub force: bool,
}

/// Generic start options for the start command
#[derive(Clone, Parser)]
pub struct GenericStartOpts {
  /// Canonical keys of the resources to start
  pub keys: Vec<String>,
}

/// Generic stop options for the stop command
#[derive(Clone, Parser)]
pub struct GenericStopOpts {
  /// Canonical keys of the resources to stop
  pub keys: Vec<String>,
}

/// Generic inspect options for the inspect command
#[derive(Clone, Parser)]
pub struct GenericInspectOpts {
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
  /// Canonical key of the object to inspect
  pub key: String,
}

/// Shared interface for browsing configuration history or execution runs.
#[derive(Clone, Parser)]
pub struct GenericHistoryOpts {
  /// Canonical resource key or global Job name
  pub key: String,
  /// Inspect one entry by its ID
  #[clap(long, value_name = "UUID", conflicts_with_all = ["limit", "offset"])]
  pub id: Option<uuid::Uuid>,
  /// Page size, 1 to 100; defaults to 50
  #[clap(long, value_parser = clap::value_parser!(i64).range(1..=100))]
  pub limit: Option<i64>,
  /// Number of entries to skip, newest first
  #[clap(long, value_parser = clap::value_parser!(i64).range(0..))]
  pub offset: Option<i64>,
  /// Display format; lists default to a table, detail to configured format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::{
    CargoCommand, Cli, Command, ContextCommand, EventCommand, JobCommand,
    MetricCommand, NamespaceCommand, NodeCommand, ResourceCommand,
    SecretCommand, VmCommand,
  };

  fn history_options(cli: Cli) -> GenericHistoryOpts {
    match cli.command {
      Command::Cargo(arg) => match arg.command {
        CargoCommand::History(opts) => opts,
        _ => panic!("expected Cargo history"),
      },
      Command::Vm(arg) => match arg.command {
        VmCommand::History(opts) => opts,
        _ => panic!("expected VM history"),
      },
      Command::Job(arg) => match arg.command {
        JobCommand::History(opts) | JobCommand::Runs(opts) => opts,
        _ => panic!("expected Job history or runs"),
      },
      _ => panic!("expected resource history"),
    }
  }

  #[test]
  fn history_commands_share_arguments_and_validation() {
    let id = "00000000-0000-0000-0000-000000000001";
    for (resource, command, key) in [
      ("cargo", "history", "global.api"),
      ("vm", "history", "global.worker"),
      ("job", "history", "daily-report"),
      ("job", "runs", "daily-report"),
    ] {
      let options = history_options(
        Cli::try_parse_from(["nanocl", resource, command, key]).unwrap(),
      );
      assert_eq!(options.key, key);
      assert!(options.id.is_none());
      assert!(options.limit.is_none());
      assert!(options.offset.is_none());
      assert!(options.display.is_none());

      let options = history_options(
        Cli::try_parse_from([
          "nanocl",
          resource,
          command,
          key,
          "--limit",
          "100",
          "--offset",
          "2",
          "--display",
          "json",
        ])
        .unwrap(),
      );
      assert_eq!(options.limit, Some(100));
      assert_eq!(options.offset, Some(2));
      assert!(matches!(options.display, Some(DisplayFormat::Json)));

      for format in ["yaml", "json", "toml"] {
        let options = history_options(
          Cli::try_parse_from([
            "nanocl",
            resource,
            command,
            key,
            "--id",
            id,
            "--display",
            format,
          ])
          .unwrap(),
        );
        assert_eq!(options.id, Some(uuid::Uuid::from_u128(1)));
        assert_eq!(options.display.unwrap().to_string(), format);
      }

      for flags in [
        vec!["--limit", "0"],
        vec!["--limit", "101"],
        vec!["--offset", "-1"],
        vec!["--id", "invalid"],
        vec!["--id", id, "--limit", "1"],
        vec!["--id", id, "--offset", "0"],
        vec!["--run", id],
      ] {
        let mut args = vec!["nanocl", resource, command, key];
        args.extend(flags);
        assert!(Cli::try_parse_from(args.clone()).is_err(), "{args:?}");
      }
    }
  }

  fn list_options(cli: Cli) -> GenericListOpts {
    match cli.command {
      Command::Cargo(arg) => match arg.command {
        CargoCommand::List(opts) => {
          assert_eq!(opts.namespace.as_deref(), Some("production"));
          opts.list
        }
        _ => panic!("expected cargo list"),
      },
      Command::Vm(arg) => match arg.command {
        VmCommand::List(opts) => {
          assert_eq!(opts.namespace.as_deref(), Some("production"));
          opts.list
        }
        _ => panic!("expected VM list"),
      },
      Command::Node(arg) => match arg.command {
        NodeCommand::List(opts) => opts,
      },
      Command::Namespace(arg) => match arg.command {
        NamespaceCommand::List(opts) => opts,
        _ => panic!("expected namespace list"),
      },
      Command::Resource(arg) => match arg.command {
        ResourceCommand::List(opts) => opts,
        _ => panic!("expected resource list"),
      },
      Command::Secret(arg) => match arg.command {
        SecretCommand::List(opts) => opts,
        _ => panic!("expected secret list"),
      },
      Command::Job(arg) => match arg.command {
        JobCommand::List(opts) => opts,
        _ => panic!("expected job list"),
      },
      Command::Event(arg) => match arg.command {
        EventCommand::List(opts) => opts,
        _ => panic!("expected event list"),
      },
      Command::Metric(arg) => match arg.command {
        MetricCommand::List(opts) => opts,
        _ => panic!("expected metric list"),
      },
      _ => panic!("expected a list command"),
    }
  }

  #[test]
  fn all_list_commands_accept_wide_with_existing_options_and_aliases() {
    for command in [
      "cargo",
      "vm",
      "job",
      "namespace",
      "node",
      "resource",
      "secret",
      "event",
      "metric",
    ] {
      for alias in ["list", "ls"] {
        let mut args = vec![
          "nanocl",
          command,
          alias,
          "--wide",
          "--quiet",
          "--limit",
          "5",
          "--offset",
          "2",
          "--filters",
          "name=app",
        ];
        if matches!(command, "cargo" | "vm") {
          args.extend(["--namespace", "production"]);
        }
        let opts = list_options(Cli::try_parse_from(args).unwrap());
        assert!(opts.wide);
        assert!(opts.quiet);
        assert_eq!(opts.limit, Some(5));
        assert_eq!(opts.offset, Some(2));
        assert_eq!(opts.filters, Some(vec!["name=app".into()]));
      }
    }
    for alias in ["list", "ls"] {
      for wide in [false, true] {
        let mut args = vec!["nanocl", "context", alias];
        if wide {
          args.push("--wide");
        }
        let Command::Context(arg) = Cli::try_parse_from(args).unwrap().command
        else {
          panic!("expected context command");
        };
        let ContextCommand::List(opts) = arg.command else {
          panic!("expected context list");
        };
        assert_eq!(opts.wide, wide);
      }
    }
    assert!(!GenericListOpts::<GenericDefaultOpts>::default().wide);
  }

  #[test]
  fn wide_list_option_does_not_change_api_filters() {
    let opts = GenericListOpts {
      wide: true,
      quiet: true,
      limit: Some(5),
      offset: Some(2),
      ..GenericListOpts::<GenericDefaultOpts>::default()
    };
    let filter: GenericFilter = opts.into();
    assert_eq!(filter.limit, Some(5));
    assert_eq!(filter.offset, Some(2));
    assert_eq!(
      serde_json::to_value(filter).unwrap(),
      serde_json::to_value(GenericFilter::default().limit(5).offset(2))
        .unwrap()
    );
  }
}
