#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use bollard_next::container::{
  LogOutput, KillContainerOptions, LogsOptions, StatsOptions,
};

pub use bollard_next::exec::CreateExecOptions;
pub use bollard_next::container::Stats as CargoStats;

use crate::node::NodeContainerSummary;
use crate::system::{Event, EventKind, ToEvent, EventAction, EventActor};

use super::cargo_config::CargoConfig;

/// A Cargo is a replicable container
/// It is used to run one or multiple instances of the same container
/// You can define the number of replicas you want to run
/// You can also define the minimum and maximum number of replicas
/// The cluster will automatically scale the number of replicas to match the number of replicas you want
/// Cargo contain a configuration which is used to create the container
/// The configuration can be updated and the old configuration will be kept in the history
/// That way you can rollback to a previous configuration quickly
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Cargo {
  /// Key of the cargo
  pub key: String,
  /// Name of the namespace
  pub namespace_name: String,
  /// Name of the cargo
  pub name: String,
  /// Unique identifier of the cargo config
  pub config_key: uuid::Uuid,
  /// Configuration of the cargo
  pub config: CargoConfig,
}

/// Convert a Cargo into an EventActor
impl From<Cargo> for EventActor {
  fn from(cargo: Cargo) -> Self {
    Self {
      key: Some(cargo.key),
      attributes: Some(serde_json::json!({
        "Name": cargo.name,
        "Namespace": cargo.namespace_name,
        "Version": cargo.config.version,
        "Metadata": cargo.config.metadata,
      })),
    }
  }
}

impl ToEvent for Cargo {
  fn to_event(&self, action: EventAction) -> Event {
    Event {
      kind: EventKind::Cargo,
      action,
      actor: Some(self.clone().into()),
    }
  }
}

/// A CargoSummary is a summary of a cargo
/// It's the datastructure returned by the list operation
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSummary {
  /// Key of the cargo
  pub key: String,
  /// Creation date of the cargo
  pub created_at: chrono::NaiveDateTime,
  /// Last update of the cargo
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the cargo
  pub name: String,
  /// Unique identifier of the cargo config
  pub config_key: uuid::Uuid,
  /// Name of the namespace
  pub namespace_name: String,
  /// Configuration of the cargo
  pub config: CargoConfig,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
}

/// Cargo Inspect is a detailed view of a cargo
/// It contains all the information about the cargo
/// It also contains the list of containers
///
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoInspect {
  /// Key of the cargo
  pub key: String,
  /// Name of the cargo
  pub name: String,
  /// Unique identifier of the cargo config
  pub config_key: uuid::Uuid,
  /// Name of the namespace
  pub namespace_name: String,
  /// Configuration of the cargo
  pub config: CargoConfig,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// List of containers
  pub instances: Vec<NodeContainerSummary>,
}

/// Kind of ExecOutput
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum OutputKind {
  /// Data is a standard input
  StdIn,
  /// Data is a standard output
  StdOut,
  /// Data is a standard error
  StdErr,
  /// Data is a console output
  Console,
}

/// ExecOutput is the output of an exec command
/// It contains the kind of the output and the data
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct OutputLog {
  /// Kind of the output
  pub kind: OutputKind,
  /// Data of the output
  pub data: String,
}

impl From<LogOutput> for OutputLog {
  fn from(output: LogOutput) -> Self {
    match output {
      LogOutput::StdOut { message } => Self {
        kind: OutputKind::StdOut,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::StdErr { message } => Self {
        kind: OutputKind::StdErr,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::Console { message } => Self {
        kind: OutputKind::Console,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::StdIn { message } => Self {
        kind: OutputKind::StdIn,
        data: String::from_utf8_lossy(&message).to_string(),
      },
    }
  }
}

/// Options for the kill command
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoKillOptions {
  /// Signal to send to the container default: SIGKILL
  pub signal: String,
}

impl Default for CargoKillOptions {
  fn default() -> Self {
    Self {
      signal: "SIGKILL".to_owned(),
    }
  }
}

impl From<CargoKillOptions> for KillContainerOptions<String> {
  fn from(options: CargoKillOptions) -> Self {
    Self {
      signal: options.signal,
    }
  }
}

/// Delete cargo query
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoDeleteQuery {
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Delete cargo even if it is running
  pub force: Option<bool>,
}

/// To use this structure for database access it needs to be able to hold a NamespaceDbModel
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct GenericCargoListQuery<NS> {
  /// Name of the namespace
  pub namespace: NS,
  /// Filter for cargoes with similar name
  pub name: Option<String>,
  /// Max amount of cargoes in response
  pub limit: Option<i64>,
  /// Offset of the first cargo in response
  pub offset: Option<i64>,
}

impl<NS> GenericCargoListQuery<NS> {
  /// Create a GenericCargoListQuery with only the namespace specified
  pub fn of_namespace(nsp: NS) -> GenericCargoListQuery<NS> {
    GenericCargoListQuery {
      namespace: nsp,
      name: None,
      limit: None,
      offset: None,
    }
  }
  /// Move fields to new query with different namespace
  pub fn merge<T>(self, nsp: T) -> GenericCargoListQuery<T> {
    GenericCargoListQuery {
      namespace: nsp,
      name: self.name,
      limit: self.limit,
      offset: self.offset,
    }
  }
}

/// List cargo query
pub type CargoListQuery = GenericCargoListQuery<Option<String>>;

/// Log cargo query
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoLogQuery {
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Only include logs since unix timestamp
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  pub until: Option<i64>,
  /// Bool, if set include timestamp to ever log line
  pub timestamps: Option<bool>,
  /// Bool, if set open the log as stream
  pub follow: Option<bool>,
  /// If integer only return last n logs, if "all" returns all logs
  pub tail: Option<String>,
  /// Include stderr in response
  pub stderr: Option<bool>,
  /// Include stdout in response
  pub stdout: Option<bool>,
}

/// Stats cargo query
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoStatsQuery {
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Stream the output. If false, the stats will be output once and then it will disconnect.
  pub stream: Option<bool>,
  /// Only get a single stat instead of waiting for 2 cycles. Must be used with `stream=false`.
  pub one_shot: Option<bool>,
}

impl From<CargoStatsQuery> for StatsOptions {
  fn from(query: CargoStatsQuery) -> StatsOptions {
    StatsOptions {
      stream: query.stream.unwrap_or(true),
      one_shot: query.one_shot.unwrap_or_default(),
    }
  }
}

impl CargoLogQuery {
  pub fn of_namespace(nsp: &str) -> CargoLogQuery {
    CargoLogQuery {
      namespace: Some(nsp.to_owned()),
      since: None,
      until: None,
      timestamps: None,
      follow: None,
      tail: None,
      stderr: None,
      stdout: None,
    }
  }
}

impl From<CargoLogQuery> for LogsOptions<String> {
  fn from(query: CargoLogQuery) -> LogsOptions<String> {
    LogsOptions::<String> {
      follow: query.follow.unwrap_or_default(),
      timestamps: query.timestamps.unwrap_or_default(),
      since: query.since.unwrap_or_default(),
      until: query.until.unwrap_or_default(),
      tail: query.tail.to_owned().unwrap_or("all".to_string()),
      stdout: query.stdout.unwrap_or(true),
      stderr: query.stdout.unwrap_or(true),
    }
  }
}

/// Payload for the cargo scale endpoint
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CargoScale {
  /// Number of replicas to scale up or down can be negative value
  pub replicas: isize,
}
