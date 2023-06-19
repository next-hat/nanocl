use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::vm::VmInspect;

/// Disk representation of a VM
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmDiskConfig {
  /// Name of the image to use
  pub image: String,
  /// Virtual size allowed for the disk
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub size: Option<u64>,
}

/// A vm's resources (cpu, memory, network)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmHostConfig {
  /// Number of cpu of the vm
  pub cpu: u64,
  /// Memory of the vm
  pub memory: u64,
  /// default network interface of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub net_iface: Option<String>,
  /// Enable KVM
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub kvm: Option<bool>,
  /// A list of DNS servers for the vm to use.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub dns: Option<Vec<String>>,
  /// Container image name to use for vm default: nexthat/nanocl-qemu
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub runtime: Option<String>,
  // Container network to use, default to the vm namespace
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub runtime_net: Option<String>,
}

impl Default for VmHostConfig {
  fn default() -> Self {
    Self {
      cpu: 1,
      memory: 512,
      net_iface: None,
      kvm: Some(false),
      dns: None,
      runtime: None,
      runtime_net: None,
    }
  }
}

/// A vm config partial is used to create a Vm
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfigPartial {
  /// Name of the vm
  pub name: String,
  /// Hostname of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default user of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Default password of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh key for the user
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// Disk config of the vm
  pub disk: VmDiskConfig,
  /// Mac address of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_config: Option<VmHostConfig>,
}

/// Payload used to patch a vm
/// It will create a new [VmConfig](VmConfig) with the new values
/// It will keep the old values in the history
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfigUpdate {
  /// Name of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// Hostname of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default user of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Default password of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh key for the user
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub host_config: Option<VmHostConfig>,
}

impl From<VmConfigPartial> for VmConfigUpdate {
  fn from(vm_config: VmConfigPartial) -> Self {
    Self {
      name: Some(vm_config.name),
      hostname: vm_config.hostname,
      user: vm_config.user,
      labels: vm_config.labels,
      host_config: vm_config.host_config,
      password: vm_config.password,
      ssh_key: vm_config.ssh_key,
    }
  }
}

/// A vm config is the configuration of a vm
/// It used to know the state of the vm
/// It keep tracking of an history when you patch an existing vm
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfig {
  /// Unique identifier of the vm config
  pub key: uuid::Uuid,
  /// Creation date of the vm config
  pub created_at: chrono::NaiveDateTime,
  /// Name of the vm
  pub name: String,
  /// Version of the config
  pub version: String,
  /// The key of the vm
  pub vm_key: String,
  /// Hostname of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub hostname: Option<String>,
  /// Default password of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub password: Option<String>,
  /// Default ssh key for the user
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ssh_key: Option<String>,
  /// Default user of the vm (cloud)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Disk config of the vm
  pub disk: VmDiskConfig,
  /// Mac address of the vm
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  pub host_config: VmHostConfig,
}

impl From<VmConfig> for VmConfigUpdate {
  fn from(vm_config: VmConfig) -> Self {
    Self {
      name: Some(vm_config.name),
      hostname: vm_config.hostname,
      user: vm_config.user,
      labels: vm_config.labels,
      host_config: Some(vm_config.host_config),
      password: vm_config.password,
      ssh_key: vm_config.ssh_key,
    }
  }
}

impl From<VmInspect> for VmConfigPartial {
  fn from(vm_inspect: VmInspect) -> Self {
    Self {
      name: vm_inspect.name,
      hostname: vm_inspect.config.hostname,
      user: vm_inspect.config.user,
      password: vm_inspect.config.password,
      ssh_key: vm_inspect.config.ssh_key,
      disk: vm_inspect.config.disk,
      mac_address: vm_inspect.config.mac_address,
      labels: vm_inspect.config.labels,
      host_config: Some(vm_inspect.config.host_config),
    }
  }
}
