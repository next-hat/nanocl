use crate::schema::{cargoes, cargo_configs};

use nanocl_models::cargo_config::CargoConfigPartial;

use super::namespace::NamespaceDbModel;

/// Structure to create a a new Cargo
pub struct CargoPartial {
  pub name: String,
  pub config: CargoConfigPartial,
}

/// Structure to create a cargo in the database
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDbModel, foreign_key = namespace_name))]
pub struct CargoDbModel {
  pub(crate) key: String,
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

/// A cargo config item is the object stored in database
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargo_configs)]
#[diesel(belongs_to(CargoDbModel, foreign_key = cargo_key))]
pub struct CargoConfigDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) cargo_key: String,
  pub(crate) config: serde_json::Value,
}
