use nanocl_stubs::cargo_spec;

use crate::schema::cargo_specs;

use super::cargo::CargoDb;

/// ## CargoSpecDb
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
#[diesel(table_name = cargo_specs)]
#[diesel(belongs_to(CargoDb, foreign_key = cargo_key))]
pub struct CargoSpecDb {
  /// The key of the cargo config
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The cargo key reference
  pub(crate) cargo_key: String,
  /// The version of the cargo config
  pub(crate) version: String,
  /// The config
  pub(crate) data: serde_json::Value,
  // The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}

impl CargoSpecDb {
  pub fn into_cargo_spec(
    self,
    config: &cargo_spec::CargoSpecPartial,
  ) -> cargo_spec::CargoSpec {
    let config = config.clone();
    cargo_spec::CargoSpec {
      key: self.key,
      created_at: self.created_at,
      name: config.name,
      version: self.version,
      cargo_key: self.cargo_key,
      init_container: config.init_container,
      replication: config.replication,
      container: config.container,
      metadata: config.metadata,
      secrets: config.secrets,
    }
  }
}
