use nanocl_stubs::vm::Vm;
use nanocl_stubs::vm_spec::VmSpec;

use crate::schema::vms;

use super::WithSpec;
use super::namespace::NamespaceDb;

/// ## VmDb
///
/// This structure represent the vm in the database.
/// A vm is a virtual machine that is running on the server.
/// The vm is linked to a namespace.
/// We use the `spec_key` to link to the vm spec.
/// The `key` is used to identify the vm and is generated as follow: `namespace_name-vm_name`.
///
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vms)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct VmDb {
  /// The key of the vm
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The name of the vm
  pub name: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
  /// The namespace name reference
  pub namespace_name: String,
}

impl WithSpec for VmDb {
  type Type = Vm;
  type Relation = VmSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

/// ## VmUpdateDb
///
/// This structure is used to update a vm in the database.
///
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
