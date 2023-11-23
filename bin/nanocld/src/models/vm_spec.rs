use nanocl_stubs::vm_spec;

use crate::schema::vm_specs;

use super::vm;

/// ## VmSpecDb
///
/// This structure represent the vm spec in the database.
/// A vm spec represent the specification of a virtual machine.
/// It is stored as a json object in the database.
/// We use the `vm_key` to link to the vm.
/// And the version is used to know which version of the spec is used
/// to ensure consistency between updates.
///
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vm_specs)]
#[diesel(belongs_to(vm::VmDb, foreign_key = vm_key))]
pub struct VmSpecDb {
  /// The key of the vm spec
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The vm key reference
  pub(crate) vm_key: String,
  /// The version of the vm spec
  pub(crate) version: String,
  /// The spec of the vm
  pub(crate) data: serde_json::Value,
  /// The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}

impl VmSpecDb {
  pub fn into_vm_spec(self, spec: &vm_spec::VmSpecPartial) -> vm_spec::VmSpec {
    vm_spec::VmSpec {
      key: self.key,
      created_at: self.created_at,
      name: spec.name.clone(),
      version: self.version,
      vm_key: self.vm_key,
      disk: spec.disk.clone(),
      host_config: spec.host_config.clone().unwrap_or_default(),
      hostname: spec.hostname.clone(),
      user: spec.user.clone(),
      labels: spec.labels.clone(),
      mac_address: spec.mac_address.clone(),
      password: spec.password.clone(),
      ssh_key: spec.ssh_key.clone(),
      metadata: spec.metadata.clone(),
    }
  }
}
