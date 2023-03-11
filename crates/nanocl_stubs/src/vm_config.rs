#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// A vm config partial is used to create a Vm
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmConfigPartial {
  /// Name of the vm
  pub name: String,
  /// Name of the image
  pub image: String,
  /// hostname of the vm
  pub hostname: Option<String>,
  /// Number of cpu of the vm
  pub cpu: Option<u64>,
  /// Memory of the vm
  pub memory: Option<u64>,
  /// default network interface of the vm
  pub net_iface: Option<String>,
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
  /// Name of the image
  pub image: String,
  /// hostname of the vm
  pub hostname: Option<String>,
  /// Number of cpu of the vm
  pub cpu: Option<u64>,
  /// Memory of the vm
  pub memory: Option<u64>,
  /// default network interface of the vm
  pub net_iface: Option<String>,
}

impl From<VmConfigPartial> for VmConfigUpdate {
  fn from(vm_config: VmConfigPartial) -> Self {
    Self {
      name: Some(vm_config.name),
      image: vm_config.image,
      hostname: vm_config.hostname,
      cpu: vm_config.cpu,
      memory: vm_config.memory,
      net_iface: vm_config.net_iface,
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
  /// Name of the image
  pub image: String,
  /// hostname of the vm
  pub hostname: Option<String>,
  /// Number of cpu of the vm
  pub cpu: Option<u64>,
  /// Memory of the vm
  pub memory: Option<u64>,
  /// default network interface of the vm
  pub net_iface: Option<String>,
}

impl From<VmConfig> for VmConfigUpdate {
  fn from(vm_config: VmConfig) -> Self {
    Self {
      name: Some(vm_config.name),
      image: vm_config.image,
      hostname: vm_config.hostname,
      cpu: vm_config.cpu,
      memory: vm_config.memory,
      net_iface: vm_config.net_iface,
    }
  }
}
