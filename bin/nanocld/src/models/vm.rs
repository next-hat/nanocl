use crate::schema::vms;

use super::namespace::NamespaceDbModel;

/// ## VmDbModel
///
/// This structure represent the vm in the database.
/// A vm is a virtual machine that is running on the server.
/// The vm is linked to a namespace.
/// We use the `config_key` to link to the vm config.
/// The `key` is used to identify the vm and is generated as follow: `namespace_name-vm_name`.
///
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vms)]
#[diesel(belongs_to(NamespaceDbModel, foreign_key = namespace_name))]
pub struct VmDbModel {
  /// The key of the vm
  pub(crate) key: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The name of the vm
  pub(crate) name: String,
  /// The config key reference
  pub(crate) config_key: uuid::Uuid,
  /// The namespace name reference
  pub(crate) namespace_name: String,
}

/// ## VmUpdateDbModel
///
/// This structure is used to update a vm in the database.
///
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = vms)]
pub struct VmUpdateDbModel {
  /// The key of the vm
  pub(crate) key: Option<String>,
  /// The namespace name reference
  pub(crate) namespace_name: Option<String>,
  /// The name of the vm
  pub(crate) name: Option<String>,
  /// The config key reference
  pub(crate) config_key: Option<uuid::Uuid>,
}
