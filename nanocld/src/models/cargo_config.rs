use crate::schema::cargo_configs;

use super::cargo::CargoDbModel;

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
