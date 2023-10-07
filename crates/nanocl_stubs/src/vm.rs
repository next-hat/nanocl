use bollard_next::service::ContainerSummary;
#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::vm_config::VmConfig;

/// A virtual machine instance
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Vm {
  /// Key of the vm
  pub key: String,
  /// Name of the namespace
  pub namespace_name: String,
  /// Name of the vm
  pub name: String,
  /// Unique identifier of the vm config
  pub config_key: uuid::Uuid,
  /// Configuration of the vm
  pub config: VmConfig,
}

/// A Vm Summary is a summary of a vm
/// It is used to list all the vms
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmSummary {
  /// Key of the vm
  pub key: String,
  /// Creation date of the vm
  pub created_at: chrono::NaiveDateTime,
  /// Update date of the vm
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the vm
  pub name: String,
  /// Unique identifier of the vm config
  pub config_key: uuid::Uuid,
  /// Name of the namespace
  pub namespace_name: String,
  /// Configuration of the vm
  pub config: VmConfig,
  /// Number of instances
  pub instances: usize,
  /// Number of running instances
  pub running_instances: usize,
}

/// A Vm Inspect is a detailed view of a cargo
/// It is used to inspect a cargo
/// It contains all the information about the cargo
/// It also contains the list of containers
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmInspect {
  /// Key of the cargo
  pub key: String,
  /// Name of the cargo
  pub name: String,
  /// Unique identifier of the cargo config
  pub config_key: uuid::Uuid,
  /// Name of the namespace
  pub namespace_name: String,
  /// Configuration of the cargo
  pub config: VmConfig,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// List of containers
  pub instances: Vec<ContainerSummary>,
}
