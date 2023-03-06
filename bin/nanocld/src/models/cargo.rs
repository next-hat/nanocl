use crate::schema::cargoes;

use super::namespace::NamespaceDbModel;

/// Structure to create a cargo in the database
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDbModel, foreign_key = namespace_name))]
pub struct CargoDbModel {
  pub(crate) key: String,
  pub(crate) created_at: chrono::NaiveDateTime,
  pub(crate) name: String,
  pub(crate) config_key: uuid::Uuid,
  pub(crate) namespace_name: String,
}

/// Structure to update a cargo in the database
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = cargoes)]
pub struct CargoUpdateDbModel {
  pub(crate) key: Option<String>,
  pub(crate) namespace_name: Option<String>,
  pub(crate) name: Option<String>,
  pub(crate) config_key: Option<uuid::Uuid>,
}

/// Structure used to serialize cargo reset path
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CargoResetPath {
  pub version: String,
  pub name: String,
  pub id: String,
}
