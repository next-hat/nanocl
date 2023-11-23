use nanocl_stubs::cargo::Cargo;
use nanocl_stubs::cargo_spec::CargoSpec;

use crate::schema::cargoes;

use super::namespace::NamespaceDb;

/// ## CargoDb
///
/// This structure represent the cargo in the database.
/// A cargo is a replicable container that can be used to deploy a service.
/// His specification is stored as a relation to a `CargoSpecDb`.
/// To keep track of the history of the cargo.
///
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct CargoDb {
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

impl CargoDb {
  pub fn into_cargo(self, spec: CargoSpec) -> Cargo {
    Cargo {
      key: self.key,
      name: self.name,
      spec_key: spec.key,
      namespace_name: self.namespace_name,
      spec,
    }
  }
}

/// ## CargoUpdateDb
///
/// This structure is used to update a cargo in the database.
///
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = cargoes)]
pub struct CargoUpdateDb {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub(crate) key: Option<String>,
  /// The name of the cargo
  pub(crate) name: Option<String>,
  /// The namespace name
  pub(crate) namespace_name: Option<String>,
  /// The spec key reference
  pub(crate) spec_key: Option<uuid::Uuid>,
}
