use nanocl_stubs::generic::GenericDelete;
use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{Pool, CargoConfigDbModel};
use crate::repositories::error::{db_blocking_error, db_error};

/// ## Create cargo config
///
/// Create a cargo config item in database for given cargo
///
/// ## Arguments
///
/// - [cargo_key](String) - Cargo key
/// - [item](CargoConfigPartial) - Cargo config item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoConfig) - The created cargo config
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::cargo_config::CargoConfigPartial;
///
/// let item = CargoConfigPartial {
///  // Fill config
/// };
/// let config = create("test".into(), item, &pool).await;
/// ```
///
pub async fn create(
  cargo_key: String,
  item: CargoConfigPartial,
  pool: &Pool,
) -> Result<CargoConfig, HttpResponseError> {
  use crate::schema::cargo_configs::dsl;

  let pool = pool.to_owned();
  let dbmodel = CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    config: serde_json::to_value(item.to_owned()).map_err(|e| {
      HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Failed to serialize config: {}", e),
      }
    })?,
  };
  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::cargo_configs)
      .values(&dbmodel)
      .execute(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpResponseError>(dbmodel)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = CargoConfig {
    key: dbmodel.key,
    name: item.name,
    cargo_key: dbmodel.cargo_key,
    dns_entry: item.dns_entry,
    replication: item.replication,
    container: item.container,
  };

  Ok(config)
}

/// ## Find cargo config by key
///
/// Find a cargo config item in database for given key
///
/// ## Arguments
///
/// - [key](uuid::Uuid) - Cargo config key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoConfig) - The found cargo config
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let config = find_by_key(uuid::Uuid::new_v4(), &pool).await;
/// ```
///
pub async fn find_by_key(
  key: uuid::Uuid,
  pool: &Pool,
) -> Result<CargoConfig, HttpResponseError> {
  use crate::schema::cargo_configs::dsl;

  let pool = pool.to_owned();
  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let config = dsl::cargo_configs
      .filter(dsl::key.eq(key))
      .get_result::<CargoConfigDbModel>(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpResponseError>(config)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = serde_json::from_value::<CargoConfigPartial>(dbmodel.config)
    .map_err(|e| HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to deserialize config: {}", e),
    })?;

  Ok(CargoConfig {
    key: dbmodel.key,
    name: config.name,
    cargo_key: dbmodel.cargo_key,
    dns_entry: config.dns_entry,
    replication: config.replication,
    container: config.container,
  })
}

/// ## Delete cargo config by cargo key
///
/// Delete all cargo config items in database for given cargo key
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
/// let res = delete_by_cargo_key(String::from("test"), &pool).await;
/// ```
///
pub async fn delete_by_cargo_key(
  key: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::cargo_configs::dsl;

  let pool = pool.to_owned();
  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::delete(dsl::cargo_configs)
      .filter(dsl::cargo_key.eq(key))
      .execute(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpResponseError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(GenericDelete { count: res })
}
