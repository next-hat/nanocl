use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::cargo::{Cargo, GenericCargoListQuery};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{
  Pool, CargoDbModel, NamespaceDbModel, CargoUpdateDbModel, CargoConfigDbModel,
};

use super::cargo_config;

/// ## Find cargo items by namespace
///
/// ## Arguments
///
/// - [nsp](NamespaceItem) - Namespace item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<CargoDbModel>) - List a cargo found
///  - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::namespace::NamespaceItem;
/// let nsp = NamespaceItem {
///  name: String::from("test"),
/// };
/// let items = find_by_namespace(nsp, &pool).await;
/// ```
///
pub async fn find_by_namespace(
  nsp: &NamespaceDbModel,
  pool: &Pool,
) -> IoResult<Vec<CargoDbModel>> {
  let query = GenericCargoListQuery::of_namespace(nsp.clone());
  find_by_list_query(&query, pool).await
}

/// ## Find cargo items by list query
///
/// ## Arguments
///
/// - [query](GenericCargoListQuery) - Query containing namespace, name filter and pagination info
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<CargoDbModel>) - List a cargo found
///  - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::namespace::NamespaceItem;
/// use nanocl_stubs::cargo::GenericCargoListQuery;
/// let nsp = NamespaceItem {
///  name: String::from("test"),
/// };
/// let query = GenericCargoListQuery::of_namespace(nsp);
/// let items = find_by_list_query(&query, &pool).await;
/// ```
///
pub async fn find_by_list_query(
  query: &GenericCargoListQuery<NamespaceDbModel>,
  pool: &Pool,
) -> IoResult<Vec<CargoDbModel>> {
  use crate::schema::cargoes::dsl;

  let query = query.clone();
  let pool = pool.clone();

  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut sql = CargoDbModel::belonging_to(&query.namespace).into_boxed();
    if let Some(name) = &query.name {
      sql = sql.filter(dsl::name.ilike(format!("%{name}%")));
    }
    if let Some(limit) = query.limit {
      sql = sql.limit(limit);
    }
    if let Some(offset) = query.offset {
      sql = sql.offset(offset);
    }
    let items = sql
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Create cargo
///
/// Create a cargo item in database for given namespace
///
/// ## Arguments
///
/// - [nsp](String) - Namespace name
/// - [item](CargoPartial) - Cargo item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo created
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::cargo::CargoConfigPartial;
///
/// let item = CargoConfigPartial {
///   //... fill required data
///   name: String::from("test"),
///   container: bollard_next::container::Config {
///     image: Some(String::from("test")),
///     ..Default::default()
///   },
///   ..Default::default()
/// };
/// let cargo = create(String::from("test"), item, &pool).await;
/// ```
///
pub async fn create(
  nsp: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Cargo> {
  use crate::schema::cargoes::dsl;

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

  let pool = pool.clone();
  let key = utils::key::gen_key(&nsp, &item.name);

  let config = cargo_config::create(&key, &item, &version, &pool).await?;

  let new_item = CargoDbModel {
    key,
    name: item.name,
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::cargoes)
      .values(&new_item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(new_item)
  })
  .await?;

  let cargo = Cargo {
    key: item.key,
    name: item.name,
    config_key: config.key,
    namespace_name: item.namespace_name,
    config,
  };

  Ok(cargo)
}

/// ## Delete cargo by key
///
/// Delete a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Cargo key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The number of deleted items
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let res = delete_by_key(String::from("test"), &pool).await;
/// ```
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::cargoes::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::delete(dsl::cargoes)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(GenericDelete { count: res })
}

/// ## Find cargo by key
///
/// Find a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Cargo key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoDbModel) - The cargo found
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let cargo = find_by_key(String::from("test"), &pool).await;
/// ```
///
pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<CargoDbModel> {
  use crate::schema::cargoes::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::cargoes
      .filter(dsl::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(item)
  })
  .await?;

  Ok(item)
}

/// ## Update cargo by key
///
/// Update a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Cargo key
/// - [item](CargoConfigPartial) - Cargo config
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo updated
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::cargo::CargoConfigPartial;
/// let item = CargoConfigPartial {
///  //... fill required data
///  name: String::from("test"),
///  container: bollard_next::container::Config {
///   image: Some(String::from("test")),
///   ..Default::default()
///  },
///  ..Default::default()
/// };
/// let cargo = update_by_key(String::from("test"), item, &pool).await;
/// ```
///
pub async fn update_by_key(
  key: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Cargo> {
  use crate::schema::cargoes::dsl;

  let key = key.to_owned();
  let item = item.clone();
  let version = version.to_owned();
  let pool = pool.clone();

  let cargodb = find_by_key(&key, &pool).await?;
  let config = cargo_config::create(&key, &item, &version, &pool).await?;

  let new_item = CargoUpdateDbModel {
    name: Some(item.name),
    config_key: Some(config.key),
    ..Default::default()
  };

  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::update(dsl::cargoes.filter(dsl::key.eq(key)))
      .set(&new_item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Cargo"))?;
    Ok::<_, IoError>(())
  })
  .await?;

  let cargo = Cargo {
    key: cargodb.key,
    name: cargodb.name,
    config_key: config.key,
    namespace_name: cargodb.namespace_name,
    config,
  };

  Ok(cargo)
}

/// ## Count cargo by namespace
///
/// Count cargo items in database for given namespace
///
/// ## Arguments
///
/// - [namespace](String) - Namespace name
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](i64) - The number of cargo items
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let count = count_by_namespace(String::from("test"), &pool
/// ).await;
/// ```
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

  let config = serde_json::from_value::<CargoConfigPartial>(item.1.config)
    .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;

  let config = CargoConfig {
    key: item.1.key,
    created_at: item.0.created_at,
    name: config.name,
    version: item.1.version,
    cargo_key: item.1.cargo_key,
    replication: config.replication,
    container: config.container,
  };

  let item = Cargo {
    key: item.0.key,
    name: item.0.name,
    config_key: item.1.key,
    namespace_name: item.0.namespace_name,
    config,
  };

  Ok(item)
}
