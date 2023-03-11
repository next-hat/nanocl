#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::vm_config::VmConfig;

/// A virtual machine instance
#[derive(Debug, Clone)]
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
