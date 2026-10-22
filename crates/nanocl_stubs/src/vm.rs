#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
  process::Process,
  system::{EventActor, EventActorKind, ObjPsStatus},
  vm_spec::{VmSpec, VmSpecPartial},
};

/// Options for stopping or restarting a VM through its private QMP socket.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase", default))]
pub struct VmLifecycleOptions {
  /// Seconds to wait for the guest to shut down cleanly (1 through 3600).
  #[cfg_attr(
    feature = "utoipa",
    schema(default = 60, minimum = 1, maximum = 3600)
  )]
  #[cfg_attr(feature = "schemars", schemars(range(min = 1, max = 3600)))]
  pub timeout: u64,
  /// Explicitly power off the VM without waiting for guest shutdown.
  #[cfg_attr(feature = "utoipa", schema(default = false))]
  pub force: bool,
}

impl Default for VmLifecycleOptions {
  fn default() -> Self {
    Self {
      timeout: 60,
      force: false,
    }
  }
}

impl VmLifecycleOptions {
  pub fn validate(&self) -> Result<(), String> {
    if !(1..=3600).contains(&self.timeout) {
      return Err(
        "VM shutdown timeout must be between 1 and 3600 seconds".to_owned(),
      );
    }
    Ok(())
  }
}

/// Current QEMU execution state, distinct from the runtime container state.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmRuntimeStatus {
  /// Whether QEMU is executing guest CPUs.
  pub running: bool,
  /// QEMU run state, or `exited` when the runtime container is stopped.
  pub status: String,
}

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
  /// Status of the vm
  pub status: ObjPsStatus,
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
  /// Status of the vm
  pub status: ObjPsStatus,
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
  /// Status of the vm
  pub status: ObjPsStatus,
  /// Specification of the vm
  pub spec: VmSpec,
  /// List of instances
  pub instances: Vec<Process>,
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::*;

  #[test]
  fn vm_lifecycle_defaults_and_timeout_bounds() {
    let defaults: VmLifecycleOptions = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults, VmLifecycleOptions::default());
    assert_eq!(
      serde_json::to_value(&defaults).unwrap(),
      serde_json::json!({"Timeout": 60, "Force": false})
    );
    let force: VmLifecycleOptions =
      serde_json::from_str(r#"{"Force":true}"#).unwrap();
    assert_eq!(force.timeout, 60);
    assert!(force.force);
    for timeout in [1, 60, 3600] {
      assert!(
        VmLifecycleOptions {
          timeout,
          force: false
        }
        .validate()
        .is_ok()
      );
    }
    for timeout in [0, 3601, u64::MAX] {
      assert!(
        VmLifecycleOptions {
          timeout,
          force: true
        }
        .validate()
        .is_err()
      );
    }
    assert!(
      serde_json::from_str::<VmLifecycleOptions>(r#"{"Timeout":-1}"#).is_err()
    );
  }

  #[test]
  fn vm_runtime_status_preserves_paused_and_exited_states() {
    for status in ["paused", "exited"] {
      let value = serde_json::json!({"Running": false, "Status": status});
      let decoded: VmRuntimeStatus =
        serde_json::from_value(value.clone()).unwrap();
      assert!(!decoded.running);
      assert_eq!(decoded.status, status);
      assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
  }
}
