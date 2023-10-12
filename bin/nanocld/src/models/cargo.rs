use nanocl_stubs::{cargo_config, cargo};

use crate::schema::cargoes;

use super::namespace::NamespaceDbModel;

/// ## CargoDbModel
///
/// This structure represent the cargo in the database.
/// A cargo is a replicable container that can be used to deploy a service.
/// His configuration is stored as a relation to a `CargoConfigDbModel`.
/// To keep track of the history of the cargo.
///
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDbModel, foreign_key = namespace_name))]
pub struct CargoDbModel {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub(crate) key: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The name of the cargo
  pub(crate) name: String,
  /// The config key reference
  pub(crate) config_key: uuid::Uuid,
  /// The namespace name
  pub(crate) namespace_name: String,
}
impl CargoDbModel {
  pub fn into_cargo(self, config: cargo_config::CargoConfig) -> cargo::Cargo {
    cargo::Cargo {
      key: self.key,
      name: self.name,
      config_key: config.key,
      namespace_name: self.namespace_name,
      config,
    }
  }
}
/// ## CargoUpdateDbModel
///
/// This structure is used to update a cargo in the database.
///
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = cargoes)]
pub struct CargoUpdateDbModel {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub(crate) key: Option<String>,
  /// The name of the cargo
  pub(crate) name: Option<String>,
  /// The namespace name
  pub(crate) namespace_name: Option<String>,
  /// The config key reference
  pub(crate) config_key: Option<uuid::Uuid>,
}

/// ## CargoRevertPath
///
/// Structure used to serialize the endpoint cargo revert path
///
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CargoRevertPath {
  /// The version
  pub version: String,
  /// The name
  pub name: String,
  /// The history id
  pub id: String,
}
