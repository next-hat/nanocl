use nanocl_stubs::{cargo, cargo_spec};

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
  /// The spec key reference
  pub(crate) spec_key: uuid::Uuid,
  /// The namespace name
  pub(crate) namespace_name: String,
}

impl CargoDbModel {
  pub fn to_cargo_with_spec(
    &self,
    spec: cargo_spec::CargoSpec,
  ) -> cargo::Cargo {
    cargo::Cargo {
      key: self.key.clone(),
      name: self.name.clone(),
      spec_key: spec.key,
      namespace_name: self.namespace_name.clone(),
      spec,
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
  /// The spec key reference
  pub(crate) spec_key: Option<uuid::Uuid>,
}
