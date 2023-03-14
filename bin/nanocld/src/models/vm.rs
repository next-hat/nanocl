use crate::schema::vms;

use super::namespace::NamespaceDbModel;

/// Structure to create a cargo in the database
#[derive(Clone, Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vms)]
#[diesel(belongs_to(NamespaceDbModel, foreign_key = namespace_name))]
pub struct VmDbModel {
  pub(crate) key: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) name: String,
  pub(crate) config_key: uuid::Uuid,
  pub(crate) namespace_name: String,
}

/// Structure to update a cargo in the database
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = vms)]
pub struct VmUpdateDbModel {
  pub(crate) key: Option<String>,
  pub(crate) namespace_name: Option<String>,
  pub(crate) name: Option<String>,
  pub(crate) config_key: Option<uuid::Uuid>,
}
