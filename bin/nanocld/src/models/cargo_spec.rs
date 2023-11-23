use nanocl_stubs::cargo_spec;

use crate::schema::cargo_specs;

use super::cargo::CargoDb;

/// ## CargoSpecDb
///
/// This structure represent the cargo spec in the database.
/// A cargo spec represent the specification of container that can be replicated.
/// It is stored as a json object in the database.
/// We use the cargo key as a foreign key to link the cargo spec to the cargo.
/// And the version is used to know which version of the spec is used
/// to ensure consistency between updates.
///
#[derive(Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargo_specs)]
#[diesel(belongs_to(CargoDb, foreign_key = cargo_key))]
pub struct CargoSpecDb {
  /// The key of the cargo spec
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The cargo key reference
  pub(crate) cargo_key: String,
  /// The version of the cargo spec
  pub(crate) version: String,
  /// The spec
  pub(crate) data: serde_json::Value,
  // The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}

impl CargoSpecDb {
  pub fn into_cargo_spec(
    self,
    spec: &cargo_spec::CargoSpecPartial,
  ) -> cargo_spec::CargoSpec {
    let spec = spec.clone();
    cargo_spec::CargoSpec {
      key: self.key,
      created_at: self.created_at,
      name: spec.name,
      version: self.version,
      cargo_key: self.cargo_key,
      init_container: spec.init_container,
      replication: spec.replication,
      container: spec.container,
      metadata: spec.metadata,
      secrets: spec.secrets,
    }
  }
}
