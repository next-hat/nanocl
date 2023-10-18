use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo::{Cargo, GenericCargoListQuery};
use nanocl_stubs::cargo_config::CargoConfigPartial;

use crate::utils;
use crate::models::{
  Pool, CargoDbModel, CargoUpdateDbModel, CargoConfigDbModel, NamespaceDbModel,
};

/// ## Find by namespace
///
/// Find a cargo by a `NamespaceDbModel` in database and return a `Vec<CargoDbModel>`.
///
/// ## Arguments
///
/// * [nsp](NamespaceDbModel) - Namespace item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///  * [Ok](Vec<CargoDbModel>) - List a cargo found
///  * [Err](IoError) - Error during the operation
///
pub async fn find_by_namespace(
  nsp: &NamespaceDbModel,
  pool: &Pool,
) -> IoResult<Vec<CargoDbModel>> {
  let query = GenericCargoListQuery::of_namespace(nsp.clone());
  list_by_query(&query, pool).await
}

/// ## List by query
///
/// List a cargo by a `GenericCargoListQuery` in database and return a `Vec<CargoDbModel>`.
///
/// ## Arguments
///
/// * [query](GenericCargoListQuery) - Query containing namespace, name filter and pagination info
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///  * [Ok](Vec<CargoDbModel>) - List a cargo found
///  * [Err](IoError) - Error during the operation
///
pub async fn list_by_query(
  query: &GenericCargoListQuery<NamespaceDbModel>,
  pool: &Pool,
) -> IoResult<Vec<CargoDbModel>> {
  use crate::schema::cargoes;
  let query = query.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut sql = CargoDbModel::belonging_to(&query.namespace).into_boxed();
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Cargo) - The cargo created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  nsp: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Cargo> {
  let nsp = nsp.to_owned();
  let item = item.to_owned();
  let version = version.to_owned();
  // test if the name of the cargo include a . in the name and throw error if true
  if item.name.contains('.') {
    return Err(IoError::invalid_input(
      "CargoConfigPartial",
      "Name cannot contain a dot.",
    ));
  }
  let key = utils::key::gen_key(&nsp, &item.name);
  let config = super::cargo_config::create(&key, &item, &version, pool).await?;
  let new_item = CargoDbModel {
    key,
    name: item.name,
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };
  let item: CargoDbModel =
    super::generic::generic_insert_with_res(pool, new_item).await?;
  let cargo = item.into_cargo(config);
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - The number of deleted items
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::cargoes;
  let key = key.to_owned();
  super::generic::generic_delete_by_id::<cargoes::table, _>(
    pool,
    key.to_owned(),
  )
  .await
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](CargoDbModel) - The cargo found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<CargoDbModel> {
  use crate::schema::cargoes;
  let key = key.to_owned();
  super::generic::generic_find_by_id::<cargoes::table, _, _>(
    pool,
    key.to_owned(),
  )
  .await
}

/// ## Update by key
///
/// Update a cargo item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [item](CargoConfigPartial) - Cargo config
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Cargo) - The cargo updated
///   * [Err](IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Cargo> {
  use crate::schema::cargoes;
  let version = version.to_owned();
  let cargodb = find_by_key(key, pool).await?;
  let config = super::cargo_config::create(key, item, &version, pool).await?;
  let new_item = CargoUpdateDbModel {
    name: Some(item.name.to_owned()),
    config_key: Some(config.key),
    ..Default::default()
  };
  let key = key.to_owned();
  super::generic::generic_update_by_id::<
    cargoes::table,
    CargoUpdateDbModel,
    _,
  >(pool, key.to_owned(), new_item)
  .await?;
  let cargo = cargodb.into_cargo(config);
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](i64) - The number of cargo items
///   * [Err](IoError) - Error during the operation
///
pub async fn count_by_namespace(nsp: &str, pool: &Pool) -> IoResult<i64> {
  use crate::schema::cargoes;
  let nsp = nsp.to_owned();
  let pool = pool.clone();
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Cargo) - The cargo found
///   * [Err](IoError) - Error during the operation
///
pub async fn inspect_by_key(key: &str, pool: &Pool) -> IoResult<Cargo> {
  use crate::schema::cargoes;
  use crate::schema::cargo_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let item: (CargoDbModel, CargoConfigDbModel) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = cargoes::table
      .inner_join(cargo_configs::table)
      .filter(cargoes::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  let config =
    serde_json::from_value::<CargoConfigPartial>(item.1.data.clone())
      .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;
  let config = item.1.into_cargo_config(&config);
  let item = item.0.into_cargo(config);
  Ok(item)
}
