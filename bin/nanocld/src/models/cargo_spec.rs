use nanocl_error::io::{IoResult, FromIo};
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::schema::cargo_specs;

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
#[diesel(table_name = cargo_specs)]
#[diesel(belongs_to(CargoDbModel, foreign_key = cargo_key))]
pub struct CargoSpecDbModel {
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

impl CargoSpecDbModel {
  pub fn to_spec_with_partial(&self, spec: &CargoSpecPartial) -> CargoSpec {
    let spec = spec.clone();
    CargoSpec {
      key: self.key,
      created_at: self.created_at,
      version: self.version.clone(),
      cargo_key: self.cargo_key.clone(),
      init_container: spec.init_container,
      replication: spec.replication,
      container: spec.container,
      metadata: self.metadata.clone(),
      secrets: spec.secrets,
    }
  }

  pub fn deserialize_data(&self) -> IoResult<CargoSpecPartial> {
    let spec = serde_json::from_value::<CargoSpecPartial>(self.data.clone())
      .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
    Ok(spec)
  }

  pub fn dezerialize_to_spec(&self) -> IoResult<CargoSpec> {
    let spec = self.deserialize_data()?;
    Ok(self.to_spec_with_partial(&spec))
  }
}
