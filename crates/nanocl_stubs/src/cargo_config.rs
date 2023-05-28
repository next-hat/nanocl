#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

pub use bollard_next::container::Config;
pub use bollard_next::models::HostConfig;
pub use bollard_next::models::HealthConfig;

use crate::cargo::CargoInspect;

/// Auto is used to automatically define that the number of replicas in the cluster
/// Number is used to manually set the number of replicas
/// Note: auto will ensure at least 1 replica exists in the cluster
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "Mode", rename_all = "PascalCase"))]
pub enum ReplicationMode {
  /// Auto is used to automatically define that the number of replicas in the cluster
  /// This will ensure at least 1 replica exists in the cluster
  /// And automatically add more replicas in the cluster if needed for redundancy
  Auto,
  /// Unique is used to ensure that only one replica exists in the cluster
  Unique,
  /// UniqueByNode is used to ensure one replica is running on each node
  UniqueByNode,
  /// UniqueByNodeGroups is used to ensure one replica is running on each node group
  UniqueByNodeGroups { groups: Vec<String> },
  /// UniqueByNodeNames is used to ensure one replica is running on each node name
  UniqueByNodeNames { names: Vec<String> },
  /// Number is used to manually set the number of replicas in one node
  Static(ReplicationStatic),
  /// NumberByNodes is used to manually set the number of replicas in each node
  StaticByNodes(ReplicationStatic),
  /// NumberByNodeGroups is used to manually set the number of replicas in each node group
  StaticByNodeGroups { groups: Vec<String>, number: i64 },
  /// NumberByNodeNames is used to manually set the number of replicas in each node name
  StaticByNodeNames { names: Vec<String>, number: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ReplicationStatic {
  pub number: i64,
}

/// A cargo config partial is used to create a Cargo
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfigPartial {
  /// Name of the cargo
  pub name: String,
  /// Replication configuration of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
  /// Container configuration of the cargo
  pub container: Config,
}

/// Payload used to patch a cargo
/// It will create a new [CargoConfig](CargoConfig) with the new values
/// It will keep the old values in the history
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfigUpdate {
  /// New name of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// New replication configuration of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub container: Option<Config>,
  /// New container configuration of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
}

impl From<CargoConfigPartial> for CargoConfigUpdate {
  fn from(cargo_config: CargoConfigPartial) -> Self {
    Self {
      name: Some(cargo_config.name),
      container: Some(cargo_config.container),
      replication: cargo_config.replication,
    }
  }
}

/// A cargo config is the configuration of a cargo
/// It used to know the state of the cargo
/// It keep tracking of an history when you patch an existing cargo
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfig {
  /// Unique identifier of the cargo config
  pub key: uuid::Uuid,
  /// Creation date of the cargo config
  pub created_at: chrono::NaiveDateTime,
  /// Name of the cargo
  pub name: String,
  /// Version of the config
  pub version: String,
  /// The key of the cargo
  pub cargo_key: String,
  /// Replication configuration of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
  /// Container configuration of the cargo
  pub container: Config,
}

impl From<CargoConfig> for CargoConfigPartial {
  fn from(cargo_config: CargoConfig) -> Self {
    Self {
      name: cargo_config.name,
      replication: cargo_config.replication,
      container: cargo_config.container,
    }
  }
}

impl From<CargoInspect> for CargoConfigPartial {
  fn from(cargo_inspect: CargoInspect) -> Self {
    Self {
      name: cargo_inspect.name,
      replication: cargo_inspect.config.replication,
      container: cargo_inspect.config.container,
    }
  }
}
