use nanocl_error::io::{IoResult, FromIo};
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::schema::cargo_specs;

use super::generic::FromSpec;
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
  pub key: uuid::Uuid,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The cargo key reference
  pub cargo_key: String,
  /// The version of the cargo spec
  pub version: String,
  /// The spec
  pub data: serde_json::Value,
  // The metadata (user defined)
  pub metadata: Option<serde_json::Value>,
}

impl FromSpec for CargoSpecDb {
  type Spec = CargoSpec;
  type SpecPartial = CargoSpecPartial;

  fn try_from_spec_partial(
    id: &str,
    version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self> {
    let data = CargoSpecDb::try_to_data(p)?;
    Ok(CargoSpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      cargo_key: id.to_owned(),
      version: version.to_owned(),
      data,
      metadata: p.metadata.clone(),
    })
  }

  fn try_to_spec(self) -> IoResult<Self::Spec> {
    let p = serde_json::from_value::<CargoSpecPartial>(self.data.clone())
      .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
    Ok(self.into_spec(&p))
  }

  fn into_spec(self, p: &Self::SpecPartial) -> Self::Spec {
    let p = p.clone();
    CargoSpec {
      key: self.key,
      created_at: self.created_at,
      name: p.name,
      version: self.version,
      cargo_key: self.cargo_key,
      init_container: p.init_container,
      replication: p.replication,
      container: p.container,
      metadata: p.metadata,
      secrets: p.secrets,
    }
  }
}
