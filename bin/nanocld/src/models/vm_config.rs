use crate::schema::vm_configs;

use super::vm::VmDbModel;

/// ## VmConfigDbModel
///
/// This structure represent the vm config in the database.
/// A vm config represent the configuration of a virtual machine.
/// It is stored as a json object in the database.
/// We use the `vm_key` to link to the vm.
/// And the version is used to know which version of the config is used
/// to ensure consistency between updates.
///
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vm_configs)]
#[diesel(belongs_to(VmDbModel, foreign_key = vm_key))]
pub struct VmConfigDbModel {
  /// The key of the vm config
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The vm key reference
  pub(crate) vm_key: String,
  /// The version of the vm config
  pub(crate) version: String,
  /// The config of the vm
  pub(crate) config: serde_json::Value,
  /// The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}
