use nanocl_models::cargo_config::{CargoConfig, CargoConfigPartial};
use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

use nanocl_models::cargo::Cargo;
use nanocl_models::generic::GenericDelete;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{
  Pool, CargoPartial, CargoDbModel, NamespaceDbModel, CargoUpdateDbModel,
  CargoConfigDbModel,
};

use super::cargo_config;
use super::error::db_blocking_error;

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
/// use nanocl_models::namespace::NamespaceItem;
/// let nsp = NamespaceItem {
///  name: String::from("test"),
/// };
/// let items = find_by_namespace(nsp, &pool).await;
/// ```
///
pub async fn find_by_namespace(
  nsp: NamespaceDbModel,
  pool: &Pool,
) -> Result<Vec<CargoDbModel>, HttpResponseError> {
  let mut conn = utils::store::get_pool_conn(pool)?;

  let items =
    web::block(move || CargoDbModel::belonging_to(&nsp).load(&mut conn))
      .await
      .map_err(db_blocking_error)?;
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
/// use nanocl_models::cargo::CargoPartial;
///
/// let item = CargoPartial {
///   name: String::from("test"),
///   //... fill required data
/// };
/// let cargo = create(String::from("test"), item, &pool).await;
/// ```
///
pub async fn create(
  nsp: String,
  item: CargoPartial,
  pool: &Pool,
) -> Result<Cargo, HttpResponseError> {
  use crate::schema::cargoes::dsl;

  let key = utils::key::gen_key(&nsp, &item.name);

  let config = cargo_config::create(key.to_owned(), item.config, pool).await?;

  let new_item = CargoDbModel {
    key,
    name: item.name,
    namespace_name: nsp,
    config_key: config.key,
  };

  let mut conn = utils::store::get_pool_conn(pool)?;
  let item = web::block(move || {
    diesel::insert_into(dsl::cargoes)
      .values(&new_item)
      .execute(&mut conn)?;
    Ok(new_item)
  })
  .await
  .map_err(db_blocking_error)?;

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
pub async fn delete_by_key(
  key: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::cargoes::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    diesel::delete(dsl::cargoes)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;
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
pub async fn find_by_key(
  key: String,
  pool: &Pool,
) -> Result<CargoDbModel, HttpResponseError> {
  use crate::schema::cargoes::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let item = web::block(move || {
    dsl::cargoes.filter(dsl::key.eq(key)).get_result(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(item)
}

/// ## Update cargo by key
///
/// Update a cargo item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Cargo key
/// - [item](CargoPartial) - Cargo item
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
/// use nanocl_models::cargo::CargoPartial;
/// let item = CargoPartial {
///  name: String::from("test"),
///  //... fill required data
/// };
/// let cargo = update_by_key(String::from("test"), item, &pool).await;
/// ```
///
pub async fn update_by_key(
  key: String,
  item: CargoPartial,
  pool: &Pool,
) -> Result<Cargo, HttpResponseError> {
  use crate::schema::cargoes::dsl;

  let config = cargo_config::create(key.to_owned(), item.config, pool).await?;

  let new_item = CargoUpdateDbModel {
    name: Some(item.name),
    config_key: Some(config.key),
    ..Default::default()
  };

  let keycopy = key.to_owned();
  let mut conn = utils::store::get_pool_conn(pool)?;
  web::block(move || {
    diesel::update(dsl::cargoes.filter(dsl::key.eq(key)))
      .set(&new_item)
      .execute(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;

  let cargodb = find_by_key(keycopy, pool).await?;

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
pub async fn count_by_namespace(
  namespace: String,
  pool: &Pool,
) -> Result<i64, HttpResponseError> {
  use crate::schema::cargoes;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let count = web::block(move || {
    cargoes::table
      .filter(cargoes::namespace_name.eq(namespace))
      .count()
      .get_result(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(count)
}

pub async fn inspect_by_key(
  key: String,
  pool: &Pool,
) -> Result<Cargo, HttpResponseError> {
  use crate::schema::cargoes;
  use crate::schema::cargo_configs;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let item: (CargoDbModel, CargoConfigDbModel) = web::block(move || {
    cargoes::table
      .inner_join(cargo_configs::table)
      .filter(cargoes::key.eq(key))
      .first(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = serde_json::from_value::<CargoConfigPartial>(item.1.config)
    .map_err(|err| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error parsing cargo config: {}", err),
    })?;

  let config = CargoConfig {
    key: item.1.key,
    name: config.name,
    cargo_key: item.1.cargo_key,
    dns_entry: config.dns_entry,
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
