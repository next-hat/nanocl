use crate::schema::cargo_configs;

use super::cargo::CargoDbModel;

/// A cargo config item is the object stored in database
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargo_configs)]
#[diesel(belongs_to(CargoDbModel, foreign_key = cargo_key))]
pub struct CargoConfigDbModel {
  /// The key of the cargo config
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The cargo key reference
  pub(crate) cargo_key: String,
  /// The version of the cargo config
  pub(crate) version: String,
  /// The config
  pub(crate) config: serde_json::Value,
}
