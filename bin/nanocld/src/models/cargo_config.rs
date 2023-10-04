use crate::schema::cargo_configs;

use super::cargo::CargoDbModel;

/// ## CargoConfigDbModel
///
/// This structure represent the cargo config in the database.
/// A cargo config represent the configuration of container that can be replicated.
/// It is stored as a json object in the database.
/// We use the cargo key as a foreign key to link the cargo config to the cargo.
/// And the version is used to know which version of the config is used
/// to ensure consistency between updates.
///
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
  // The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}
