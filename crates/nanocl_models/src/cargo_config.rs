#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

pub type ContainerConfig<T> = bollard::container::Config<T>;
pub type ContainerHostConfig = bollard::models::HostConfig;

/// Auto is used to automatically define that the number of replicas in the cluster
/// Number is used to manually set the number of replicas
/// Note: auto will ensure at least 1 replica exists in the cluster
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
#[cfg_attr(feature = "dev", derive(ToSchema))]
pub enum ReplicaValue {
  /// Number of replicas wanted
  #[cfg_attr(feature = "serde", serde(rename = "number"))]
  Number(i64),
  /// Automatically scale the number of replicas
  #[cfg_attr(feature = "serde", serde(rename = "auto"))]
  Auto,
}

/// Cargo replication is used to define the minimum and maximum number of replicas in the cluster
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoReplication {
  /// Minimum number of replicas
  pub min_replicas: Option<i64>,
  /// Maximum number of replicas
  pub max_replicas: Option<i64>,
}

/// A cargo config partial is used to create a Cargo
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfigPartial {
  /// Name of the cargo
  pub name: String,
  /// DNS entry of the cargo
  pub dns_entry: Option<String>,
  /// Replication configuration of the cargo
  pub replication: Option<CargoReplication>,
  /// Container configuration of the cargo
  pub container: ContainerConfig<String>,
}

/// Payload used to patch a cargo
/// It will create a new [CargoConfig](CargoConfig) with the new values
/// It will keep the old values in the history
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfigPatch {
  /// New name of the cargo
  pub name: Option<String>,
  /// New DNS entry of the cargo
  pub dns_entry: Option<String>,
  /// New replication configuration of the cargo
  pub container: Option<bollard::container::Config<String>>,
  /// New container configuration of the cargo
  pub replication: Option<CargoReplication>,
}

impl From<CargoConfigPartial> for CargoConfigPatch {
  fn from(cargo_config: CargoConfigPartial) -> Self {
    Self {
      name: Some(cargo_config.name),
      dns_entry: cargo_config.dns_entry,
      container: Some(cargo_config.container),
      replication: cargo_config.replication,
    }
  }
}

/// A cargo config is the configuration of a cargo
/// It used to know the state of the cargo
/// It keep tracking of an history when you patch an existing cargo
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoConfig {
  /// Unique identifier of the cargo config
  pub key: uuid::Uuid,
  /// Name of the cargo
  pub name: String,
  /// The key of the cargo
  pub cargo_key: String,
  /// DNS entry of the cargo
  pub dns_entry: Option<String>,
  /// Replication configuration of the cargo
  pub replication: Option<CargoReplication>,
  /// Container configuration of the cargo
  pub container: ContainerConfig<String>,
}

impl From<CargoConfig> for CargoConfigPatch {
  fn from(cargo_config: CargoConfig) -> Self {
    Self {
      name: Some(cargo_config.name),
      dns_entry: cargo_config.dns_entry,
      container: Some(cargo_config.container),
      replication: cargo_config.replication,
    }
  }
}
