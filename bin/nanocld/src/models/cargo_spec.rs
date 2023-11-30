use std::sync::Arc;
use std::collections::HashMap;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocl_stubs::generic::{GenericFilter, GenericClause};
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::schema::cargo_specs;

use crate::{utils, gen_where4json, gen_where4string};
use super::{Pool, Repository, FromSpec, CargoDb};

/// This structure represent the cargo spec in the database.
/// A cargo spec represent the specification of container that can be replicated.
/// It is stored as a json object in the database.
/// We use the cargo key as a foreign key to link the cargo spec to the cargo.
/// And the version is used to know which version of the spec is used
/// to ensure consistency between updates.
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

  fn get_data(&self) -> &serde_json::Value {
    &self.data
  }

  fn to_spec(&self, p: &Self::SpecPartial) -> Self::Spec {
    let p = p.clone();
    CargoSpec {
      key: self.key,
      created_at: self.created_at,
      name: p.name,
      version: self.version.clone(),
      cargo_key: self.cargo_key.clone(),
      init_container: p.init_container,
      replication: p.replication,
      container: p.container,
      metadata: p.metadata,
      secrets: p.secrets,
    }
  }
}

impl Repository for CargoSpecDb {
  type Table = cargo_specs::table;
  type Item = CargoSpec;
  type UpdateItem = CargoSpecDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    let mut query = cargo_specs::dsl::cargo_specs.into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    if let Some(value) = r#where.get("CargoKey") {
      gen_where4string!(query, cargo_specs::dsl::cargo_key, value);
    }
    if let Some(value) = r#where.get("Version") {
      gen_where4string!(query, cargo_specs::dsl::version, value);
    }
    if let Some(value) = r#where.get("Data") {
      gen_where4json!(query, cargo_specs::dsl::data, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_result::<CargoSpecDb>(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?
        .try_to_spec()?;
      Ok::<_, IoError>(items)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    let mut query = cargo_specs::dsl::cargo_specs
      .order(cargo_specs::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    if let Some(value) = r#where.get("CargoKey") {
      gen_where4string!(query, cargo_specs::dsl::cargo_key, value);
    }
    if let Some(value) = r#where.get("Version") {
      gen_where4string!(query, cargo_specs::dsl::version, value);
    }
    if let Some(value) = r#where.get("Data") {
      gen_where4json!(query, cargo_specs::dsl::data, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<CargoSpecDb>(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?
        .into_iter()
        .map(|item| {
          let spec = item.try_to_spec()?;
          Ok::<_, IoError>(spec)
        })
        .collect::<IoResult<Vec<CargoSpec>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl CargoSpecDb {
  pub(crate) async fn find_by_cargo(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<CargoSpec>> {
    let mut r#where = HashMap::new();
    r#where.insert("CargoKey".to_owned(), GenericClause::Eq(name.to_owned()));
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    CargoSpecDb::find(&filter, pool).await?
  }
}
