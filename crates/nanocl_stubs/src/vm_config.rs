use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Disk representation of a VM
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmDiskConfig {
  /// Name of the image to use
  pub image: String,
  /// Virtual size allowed for the disk
  pub size: Option<u64>,
}

/// A vm's resources (cpu, memory, network)
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmHostConfig {
  /// Number of cpu of the vm
  pub cpu: u64,
  /// Memory of the vm
  pub memory: u64,
  /// default network interface of the vm
  pub net_iface: Option<String>,
  /// Enable KVM
  pub kvm: Option<bool>,
  /// A list of DNS servers for the vm to use.
  pub dns: Option<Vec<String>>,
  /// Runtime to use
  pub runtime: Option<String>,
}

impl Default for VmHostConfig {
  fn default() -> Self {
    Self {
      cpu: 1,
      memory: 512,
      net_iface: Some(String::default()),
      kvm: Some(true),
      dns: Some(vec![]),
      runtime: Some(String::default()),
    }
  }
}

/// A vm config partial is used to create a Vm
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfigPartial {
  /// Name of the vm
  pub name: String,
  /// Hostname of the vm
  pub hostname: Option<String>,
  /// Domain name of the vm
  pub domainname: Option<String>,
  /// Default user of the vm (cloud)
  pub user: Option<String>,
  /// Disk config of the vm
  pub disk: VmDiskConfig,
  /// Mac address of the vm
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  pub host_config: Option<VmHostConfig>,
}

/// Payload used to patch a vm
/// It will create a new [VmConfig](VmConfig) with the new values
/// It will keep the old values in the history
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfigUpdate {
  /// Name of the vm
  pub name: Option<String>,
  /// Hostname of the vm
  pub hostname: Option<String>,
  /// Domain name of the vm
  pub domainname: Option<String>,
  /// Default user of the vm (cloud)
  pub user: Option<String>,
  /// Disk config of the vm
  pub disk: Option<VmDiskConfig>,
  /// Mac address of the vm
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  pub host_config: Option<VmHostConfig>,
}

impl From<VmConfigPartial> for VmConfigUpdate {
  fn from(vm_config: VmConfigPartial) -> Self {
    Self {
      name: Some(vm_config.name),
      hostname: vm_config.hostname,
      domainname: vm_config.domainname,
      user: vm_config.user,
      disk: Some(vm_config.disk),
      mac_address: vm_config.mac_address,
      labels: vm_config.labels,
      host_config: vm_config.host_config,
    }
  }
}

/// A vm config is the configuration of a vm
/// It used to know the state of the vm
/// It keep tracking of an history when you patch an existing vm
#[derive(Debug, Default, Clone)]
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
  pub hostname: Option<String>,
  /// Domain name of the vm
  pub domainname: Option<String>,
  /// Default user of the vm (cloud)
  pub user: Option<String>,
  /// Disk config of the vm
  pub disk: VmDiskConfig,
  /// Mac address of the vm
  pub mac_address: Option<String>,
  /// User-defined key/value metadata.
  pub labels: Option<HashMap<String, String>>,
  /// A vm's resources (cpu, memory, network)
  pub host_config: VmHostConfig,
}

impl From<VmConfig> for VmConfigUpdate {
  fn from(vm_config: VmConfig) -> Self {
    Self {
      name: Some(vm_config.name),
      hostname: vm_config.hostname,
      domainname: vm_config.domainname,
      user: vm_config.user,
      disk: Some(vm_config.disk),
      mac_address: vm_config.mac_address,
      labels: vm_config.labels,
      host_config: Some(vm_config.host_config),
    }
  }
}
