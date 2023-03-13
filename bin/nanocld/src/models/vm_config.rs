use crate::schema::vm_configs;

use super::vm::VmDbModel;

/// A cargo config item is the object stored in database
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = vm_configs)]
#[diesel(belongs_to(VmDbModel, foreign_key = vm_key))]
pub struct VmConfigDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) vm_key: String,
  pub(crate) version: String,
  pub(crate) config: serde_json::Value,
}
