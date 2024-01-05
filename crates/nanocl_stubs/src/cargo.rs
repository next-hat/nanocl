#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use bollard_next::container::{StatsOptions, KillContainerOptions};

use crate::{
  system::{EventActor, EventActorKind},
  cargo_spec::CargoSpecPartial,
  process::Process,
};

use super::cargo_spec::CargoSpec;

// Rexport some stuff from simplicity
pub use bollard_next::exec::CreateExecOptions;
pub use bollard_next::container::Stats as CargoStats;

/// A Cargo is a replicable container
/// It is used to run one or multiple instances of the same container
/// You can define the number of replicas you want to run
/// You can also define the minimum and maximum number of replicas
/// The cluster will automatically scale the number of replicas to match the number of replicas you want
/// Cargo contain a specification which is used to create the container
/// The specification can be updated and the old specification will be kept in the history
/// That way you can rollback to a previous specification quickly
#[derive(Debug, Clone)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Cargo {
  /// Name of the namespace
  pub namespace_name: String,
  /// When the cargo was created
  pub created_at: chrono::NaiveDateTime,
  /// Specification of the cargo
  pub spec: CargoSpec,
}

impl From<Cargo> for CargoSpecPartial {
  fn from(cargo: Cargo) -> Self {
    cargo.spec.into()
  }
}

/// Convert a Cargo into an EventActor
impl From<Cargo> for EventActor {
  fn from(cargo: Cargo) -> Self {
    Self {
      key: Some(cargo.spec.cargo_key),
      kind: EventActorKind::Cargo,
      attributes: Some(serde_json::json!({
        "Name": cargo.spec.name,
        "Namespace": cargo.namespace_name,
        "Version": cargo.spec.version,
        "Metadata": cargo.spec.metadata,
      })),
    }
  }
}

/// A CargoSummary is a summary of a cargo
/// It's the datastructure returned by the list operation
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSummary {
  /// Name of the namespace
  pub namespace_name: String,
  /// When the cargo was created
  pub created_at: chrono::NaiveDateTime,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// Specification of the cargo
  pub spec: CargoSpec,
}

/// Cargo Inspect is a detailed view of a cargo
/// It contains all the information about the cargo
/// It also contains the list of containers
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoInspect {
  /// Name of the namespace
  pub namespace_name: String,
  /// When the cargo was created
  pub created_at: chrono::NaiveDateTime,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// Specification of the cargo
  pub spec: CargoSpec,
  /// List of instances
  pub instances: Vec<Process>,
}

/// Options for the kill command
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CargoDeleteQuery {
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Delete cargo even if it is running
  pub force: Option<bool>,
}

/// Stats cargo query
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
