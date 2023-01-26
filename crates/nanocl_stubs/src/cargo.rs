#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use bollard::models::ContainerSummary;

use super::cargo_config::CargoConfig;

/// A Cargo is a replicable container
/// It is used to run one or multiple instances of the same container
/// You can define the number of replicas you want to run
/// You can also define the minimum and maximum number of replicas
/// The cluster will automatically scale the number of replicas to match the number of replicas you want
/// Cargo contain a configuration which is used to create the container
/// The configuration can be updated and the old configuration will be kept in the history
/// That way you can rollback to a previous configuration quickly
#[derive(Debug, Clone)]
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

/// A Cargo Summary is a summary of a cargo
/// It is used to list all the cargos
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSummary {
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
  /// Number of running instances
  pub running_instances: i64,
}

/// A Cargo Inspect is a detailed view of a cargo
/// It is used to inspect a cargo
/// It contains all the information about the cargo
/// It also contains the list of containers
#[derive(Default, Debug)]
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
  /// Number of running instances
  pub running_instances: i64,
  /// List of containers
  pub containers: Vec<ContainerSummary>,
}
