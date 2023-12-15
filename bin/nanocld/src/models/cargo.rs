use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::{CargoSpec, CargoSpecPartial},
  generic::{GenericFilter, GenericClause},
};

use crate::{utils, gen_where4string, schema::cargoes};

use super::{Pool, CargoSpecDb, NamespaceDb, Repository, WithSpec, FromSpec};

/// This structure represent the cargo in the database.
/// A cargo is a replicable container that can be used to deploy a service.
/// His specification is stored as a relation to a `CargoSpecDb`.
/// To keep track of the history of the cargo.
#[derive(Debug, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = cargoes)]
#[diesel(belongs_to(NamespaceDb, foreign_key = namespace_name))]
pub struct CargoDb {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The name of the cargo
  pub name: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
  /// The namespace name
  pub namespace_name: String,
}

/// This structure is used to update a cargo in the database.
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = cargoes)]
pub struct CargoUpdateDb {
  /// The key of the cargo generated with `namespace_name` and `name`
  pub key: Option<String>,
  /// The name of the cargo
  pub name: Option<String>,
  /// The namespace name
  pub namespace_name: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}

impl WithSpec for CargoDb {
  type Type = Cargo;
  type Relation = CargoSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

impl Repository for CargoDb {
  type Table = cargoes::table;
  type Item = Cargo;
  type UpdateItem = CargoUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("CargoDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::cargo_specs::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(Self, CargoSpecDb)>(&mut conn)
        .map_err(Self::map_err_context)?;
      let item = item.0.with_spec(&item.1.try_to_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("CargoDb::find filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = cargoes::dsl::cargoes
      .inner_join(crate::schema::cargo_specs::table)
      .order(cargoes::dsl::created_at.desc())
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, cargoes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, cargoes::dsl::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, cargoes::dsl::namespace_name, value);
    }
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(Self, CargoSpecDb)>(&mut conn)
        .map_err(Self::map_err_context)?;
      let items = items
        .into_iter()
        .map(|item| {
          let spec = &item.1.try_to_spec()?;
          Ok::<_, IoError>(item.0.with_spec(spec))
        })
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl CargoDb {
  /// Create a new cargo from its specification.
  pub(crate) async fn create_from_spec(
    nsp: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let nsp = nsp.to_owned();
    let item = item.to_owned();
    let version = version.to_owned();
    // test if the name of the cargo include a . in the name and throw error if true
    if item.name.contains('.') {
      return Err(IoError::invalid_input(
        "CargoSpecPartial",
        "Name cannot contain a dot.",
      ));
    }
    let key = utils::key::gen_key(&nsp, &item.name);
    let new_spec = CargoSpecDb::try_from_spec_partial(&key, &version, &item)?;
    let spec = CargoSpecDb::create(new_spec, pool).await??.try_to_spec()?;
    let new_item = CargoDb {
      key,
      name: item.name,
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: nsp,
      spec_key: spec.key,
    };
    let item = CargoDb::create(new_item, pool).await??;
    let cargo = item.with_spec(&spec);
    Ok(cargo)
  }

  /// Update a cargo from its specification.
  pub(crate) async fn update_from_spec(
    key: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let version = version.to_owned();
    let cargo = CargoDb::find_by_pk(key, pool).await??;
    let new_spec = CargoSpecDb::try_from_spec_partial(key, &version, item)?;
    let spec = CargoSpecDb::create(new_spec, pool).await??.try_to_spec()?;
    let new_item = CargoUpdateDb {
      name: Some(item.name.to_owned()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    CargoDb::update_by_pk(key, new_item, pool).await??;
    let cargo = cargo.with_spec(&spec);
    Ok(cargo)
  }

  /// Find a cargo by its key.
  pub(crate) async fn inspect_by_pk(key: &str, pool: &Pool) -> IoResult<Cargo> {
    let filter =
      GenericFilter::new().r#where("key", GenericClause::Eq(key.to_owned()));
    Self::find_one(&filter, pool).await?
  }

  /// Find cargoes by namespace.
  pub(crate) async fn find_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Cargo>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    CargoDb::find(&filter, pool).await?
  }

  /// Count cargoes by namespace.
  pub(crate) async fn count_by_namespace(
    nsp: &str,
    pool: &Pool,
  ) -> IoResult<i64> {
    let nsp = nsp.to_owned();
    let pool = Arc::clone(pool);
    let count = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let count = cargoes::table
        .filter(cargoes::namespace_name.eq(nsp))
        .count()
        .get_result(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(count)
    })
    .await?;
    Ok(count)
  }
}
