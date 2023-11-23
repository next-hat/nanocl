use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo::{Cargo, GenericCargoListQuery};
use nanocl_stubs::cargo_spec::CargoSpecPartial;

use crate::utils;
use crate::models::{Pool, CargoDb, CargoUpdateDb, CargoSpecDb, NamespaceDb};

/// ## Find by namespace
///
/// Find a cargo by a `NamespaceDb` in database and return a `Vec<CargoDb>`.
///
/// ## Arguments
///
/// * [nsp](NamespaceDb) - Namespace item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [CargoDb](CargoDb)
///
pub(crate) async fn find_by_namespace(
  nsp: &NamespaceDb,
  pool: &Pool,
) -> IoResult<Vec<CargoDb>> {
  let query = GenericCargoListQuery::of_namespace(nsp.clone());
  list_by_query(&query, pool).await
}

/// ## List by query
///
/// List a cargo by a `GenericCargoListQuery` in database and return a `Vec<CargoDb>`.
///
/// ## Arguments
///
/// * [query](GenericCargoListQuery) - Query containing namespace, name filter and pagination info
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [CargoDb](CargoDb)
///
pub(crate) async fn list_by_query(
  query: &GenericCargoListQuery<NamespaceDb>,
  pool: &Pool,
) -> IoResult<Vec<CargoDb>> {
  use crate::schema::cargoes;
  let query = query.clone();
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut sql = CargoDb::belonging_to(&query.namespace).into_boxed();
    if let Some(name) = &query.name {
      sql = sql.filter(cargoes::dsl::name.ilike(format!("%{name}%")));
    }
    if let Some(limit) = query.limit {
      sql = sql.limit(limit);
    }
    if let Some(offset) = query.offset {
      sql = sql.offset(offset);
    }
    let items = sql
      .order(cargoes::dsl::created_at.desc())
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Create
///
/// Create a cargo item in database for given namespace
///
/// ## Arguments
///
/// * [nsp](str) - Namespace name
/// * [item](Cargo) - Cargo item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Cargo](Cargo)
///
pub(crate) async fn create(
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
  let spec = super::cargo_spec::create(&key, &item, &version, pool).await?;
  let new_item = CargoDb {
    key,
    name: item.name,
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    spec_key: spec.key,
  };
  let item: CargoDb = super::generic::insert_with_res(new_item, pool).await?;
  let cargo = item.into_cargo(spec);
  Ok(cargo)
}

/// ## Delete by key
///
/// Delete a cargo item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::cargoes;
  let key = key.to_owned();
  super::generic::delete_by_id::<cargoes::table, _>(key, pool).await
}

/// ## Find by key
///
/// Find a cargo item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [CargoDb](CargoDb)
///
pub(crate) async fn find_by_key(key: &str, pool: &Pool) -> IoResult<CargoDb> {
  use crate::schema::cargoes;
  let key = key.to_owned();
  super::generic::find_by_id::<cargoes::table, _, _>(key, pool).await
}

/// ## Update by key
///
/// Update a cargo item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [item](CargoSpecPartial) - Cargo spec
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Cargo](Cargo)
///
pub(crate) async fn update_by_key(
  key: &str,
  item: &CargoSpecPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Cargo> {
  use crate::schema::cargoes;
  let version = version.to_owned();
  let cargodb = find_by_key(key, pool).await?;
  let spec = super::cargo_spec::create(key, item, &version, pool).await?;
  let new_item = CargoUpdateDb {
    name: Some(item.name.to_owned()),
    spec_key: Some(spec.key),
    ..Default::default()
  };
  let key = key.to_owned();
  super::generic::update_by_id::<cargoes::table, CargoUpdateDb, _>(
    key, new_item, pool,
  )
  .await?;
  let cargo = cargodb.into_cargo(spec);
  Ok(cargo)
}

/// ## Count by namespace
///
/// Count cargo items in database for given namespace
///
/// ## Arguments
///
/// * [namespace](str) - Namespace name
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [i64](i64)
///
pub(crate) async fn count_by_namespace(
  nsp: &str,
  pool: &Pool,
) -> IoResult<i64> {
  use crate::schema::cargoes;
  let nsp = nsp.to_owned();
  let pool = Arc::clone(pool);
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = cargoes::table
      .filter(cargoes::namespace_name.eq(nsp))
      .count()
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(count)
  })
  .await?;
  Ok(count)
}

/// ## Inspect by key
///
/// Inspect a cargo item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Cargo](Cargo)
///
pub(crate) async fn inspect_by_key(key: &str, pool: &Pool) -> IoResult<Cargo> {
  use crate::schema::cargoes;
  use crate::schema::cargo_specs;
  let key = key.to_owned();
  let pool = Arc::clone(pool);
  let item: (CargoDb, CargoSpecDb) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = cargoes::table
      .inner_join(cargo_specs::table)
      .filter(cargoes::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  let spec = serde_json::from_value::<CargoSpecPartial>(item.1.data.clone())
    .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
  let spec = item.1.into_cargo_spec(&spec);
  let item = item.0.into_cargo(spec);
  Ok(item)
}
