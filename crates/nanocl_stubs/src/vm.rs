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

/// Grow QEMU resources without restarting the VM. Multiple changes are not atomic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(rename_all = "PascalCase", deny_unknown_fields)
)]
pub struct VmResizeOptions {
  /// Requested total vCPU count; must not decrease the current count.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(minimum = 1))]
  pub cpu: Option<u64>,
  /// Requested total QEMU memory in MiB; must not decrease current memory.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(minimum = 1))]
  pub memory: Option<u64>,
  /// Requested total boot disk size in bytes; guest partitions and filesystems are unchanged.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(minimum = 1))]
  pub disk_size: Option<u64>,
}

impl VmResizeOptions {
  pub fn validate(&self) -> Result<(), String> {
    if self.cpu.is_none() && self.memory.is_none() && self.disk_size.is_none() {
      return Err(
        "Specify at least one of Cpu, Memory, or DiskSize".to_owned(),
      );
    }
    if [self.cpu, self.memory, self.disk_size]
      .into_iter()
      .flatten()
      .any(|value| value == 0)
    {
      return Err(
        "Requested VM resources must be greater than zero".to_owned(),
      );
    }
    Ok(())
  }
}

/// Resources observed in QEMU, not guest-online CPUs, memory, or filesystem capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct VmResources {
  /// Current QEMU vCPU count.
  pub cpu: u64,
  /// Current QEMU memory in MiB.
  pub memory: u64,
  /// Current boot disk size in bytes.
  pub disk_size: u64,
  /// Maximum vCPU count reserved when this runtime was created.
  pub max_cpu: u64,
  /// Maximum memory in MiB reserved when this runtime was created.
  pub max_memory: u64,
  /// Whether observed CPU and memory match both the saved specification and restart configuration.
  pub persistent: bool,
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
  fn vm_resize_options_require_positive_requested_resources() {
    assert!(VmResizeOptions::default().validate().is_err());
    for value in [
      serde_json::json!({"Cpu": 2}),
      serde_json::json!({"Memory": 1024}),
      serde_json::json!({"DiskSize": 4294967296_u64}),
      serde_json::json!({"Cpu": 4, "Memory": 2048, "DiskSize": 8589934592_u64}),
    ] {
      let options: VmResizeOptions =
        serde_json::from_value(value.clone()).unwrap();
      assert!(options.validate().is_ok());
      assert_eq!(serde_json::to_value(options).unwrap(), value);
    }
    for value in [
      serde_json::json!({"Cpu": 0}),
      serde_json::json!({"Memory": 0}),
      serde_json::json!({"DiskSize": 0}),
    ] {
      assert!(
        serde_json::from_value::<VmResizeOptions>(value)
          .unwrap()
          .validate()
          .is_err()
      );
    }
    assert!(
      serde_json::from_str::<VmResizeOptions>(r#"{"DiskSize":-1}"#).is_err()
    );
    assert!(
      serde_json::from_str::<VmResizeOptions>(r#"{"MaxCpu":4}"#).is_err()
    );
  }

  #[test]
  fn vm_resources_preserve_observed_capacity_and_persistence() {
    let value = serde_json::json!({"Cpu": 4, "Memory": 2048, "DiskSize": 8589934592_u64, "MaxCpu": 8, "MaxMemory": 8192, "Persistent": false});
    let resources: VmResources = serde_json::from_value(value.clone()).unwrap();
    assert!(!resources.persistent);
    assert_eq!(serde_json::to_value(resources).unwrap(), value);
  }

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
