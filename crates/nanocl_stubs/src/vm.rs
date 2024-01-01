#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::{
  process::Process,
  vm_spec::{VmSpec, VmSpecPartial},
  system::{EventActor, EventActorKind},
};

/// A virtual machine instance
#[derive(Debug, Clone)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Vm {
  /// Name of the namespace
  pub namespace_name: String,
  /// When the vm was created
  pub created_at: chrono::NaiveDateTime,
  /// Specification of the vm
  pub spec: VmSpec,
}

impl From<Vm> for VmSpecPartial {
  fn from(vm: Vm) -> Self {
    vm.spec.into()
  }
}

/// Convert a Vm into an EventActor
impl From<Vm> for EventActor {
  fn from(vm: Vm) -> Self {
    Self {
      key: Some(vm.spec.vm_key),
      kind: EventActorKind::Vm,
      attributes: Some(serde_json::json!({
        "Name": vm.spec.name,
        "Namespace": vm.namespace_name,
        "Version": vm.spec.version,
        "Namespace": vm.namespace_name,
        "Metadata": vm.spec.metadata,
      })),
    }
  }
}

/// A Vm Summary is a summary of a vm
/// It is used to list all the vms
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmSummary {
  /// Name of the namespace
  pub namespace_name: String,
  /// Creation date of the vm
  pub created_at: chrono::NaiveDateTime,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// Specification of the vm
  pub spec: VmSpec,
}

/// A Vm Inspect is a detailed view of a vm
/// It is used to inspect a vm
/// It contains all the information about the vm
/// It also contains the list of containers
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmInspect {
  /// Name of the namespace
  pub namespace_name: String,
  /// Creation date of the vm
  pub created_at: chrono::NaiveDateTime,
  /// Number of instances
  pub instance_total: usize,
  /// Number of running instances
  pub instance_running: usize,
  /// Specification of the vm
  pub spec: VmSpec,
  /// List of instances
  pub instances: Vec<Process>,
}
