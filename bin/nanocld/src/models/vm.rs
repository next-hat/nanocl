use diesel::prelude::*;
use nanocl_stubs::vm_spec::{VmSpecPartial, VmSpecUpdate};

use crate::schema::vms;

use super::NamespaceDb;

/// This structure represent the vm in the database.
/// A vm is a virtual machine that is running on the server.
/// The vm is linked to a namespace.
/// We use the `spec_key` to link to the vm spec.
/// The `key` identifies the VM in `{namespace}.{name}` format.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vms)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct VmDb {
  /// The key of the vm
  pub key: String,
  /// The name of the vm
  pub name: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The namespace name reference
  pub namespace_name: String,
  /// The status key
  pub status_key: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
  /// Immutable identity retained after this live definition is deleted.
  pub incarnation_key: uuid::Uuid,
}

#[derive(Debug)]
pub(crate) struct AcceptedVmOperation {
  pub vm: nanocl_stubs::vm::Vm,
  pub operation: super::OperationDb,
}

pub(crate) type VmOperationLease =
  (std::sync::Arc<VmLifecycleGuard>, super::OperationGuard);

/// This structure is used to update a vm in the database.
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = vms)]
pub struct VmUpdateDb {
  /// The key of the vm
  pub key: Option<String>,
  /// The namespace name reference
  pub namespace_name: Option<String>,
  /// The name of the vm
  pub name: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}

/// Arguments to create a new vm obj
pub struct VmObjCreateIn {
  pub namespace: String,
  pub spec: VmSpecPartial,
  pub version: String,
}

/// Arguments to create a new vm history entry
pub struct VmObjPutIn {
  pub spec: VmSpecPartial,
  pub version: String,
}

/// Arguments to update a vm history entry
pub struct VmObjPatchIn {
  pub spec: VmSpecUpdate,
  pub version: String,
}

/// Local serialization of a VM's sockets, runtime and desired specification.
/// Weak entries are pruned so deleted VM names do not accumulate forever.
pub(crate) type VmLifecycleLocks = std::sync::Mutex<
  std::collections::HashMap<String, std::sync::Weak<VmLifecycleLock>>,
>;

/// Mutation admission is separate from Docker observation serialization.
#[derive(Default)]
pub(crate) struct VmLifecycleLock {
  pub key: String,
  pub mutation: std::sync::Arc<tokio::sync::Mutex<()>>,
  pub observation: std::sync::Arc<tokio::sync::Mutex<()>>,
}

/// Keep the weak registry entry alive until both protected sections finish.
pub(crate) struct VmLifecycleGuard {
  pub _observation: tokio::sync::OwnedMutexGuard<()>,
  pub _mutation: tokio::sync::OwnedMutexGuard<()>,
  pub _lock: std::sync::Arc<VmLifecycleLock>,
}

pub(crate) struct VmLifecycleObservationGuard {
  pub _observation: tokio::sync::OwnedMutexGuard<()>,
  pub _lock: std::sync::Arc<VmLifecycleLock>,
}

/// Private restart projection for the QEMU generation mounted by Docker.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct VmRuntimeResources {
  pub cpu: u64,
  pub memory: u64,
  pub max_cpu: u64,
  pub max_memory: u64,
}
