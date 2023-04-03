use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, CargoConfigDbModel};
use super::error::{db_error, db_blocking_error};

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
  cargo_key: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> Result<CargoConfig, HttpError> {
  use crate::schema::cargo_configs::dsl;

  let cargo_key = cargo_key.to_owned();
  let version = version.to_owned();

  let pool = pool.clone();
  let dbmodel = CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    config: serde_json::to_value(item.clone()).map_err(|e| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to serialize config: {e}"),
    })?,
  };
  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::cargo_configs)
      .values(&dbmodel)
      .execute(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpError>(dbmodel)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = CargoConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: item.name.clone(),
    version: dbmodel.version,
    cargo_key: dbmodel.cargo_key,
    replication: item.replication.clone(),
    container: item.container.clone(),
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
  key: &uuid::Uuid,
  pool: &Pool,
) -> Result<CargoConfig, HttpError> {
  use crate::schema::cargo_configs::dsl;

  let key = *key;
  let pool = pool.clone();

  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let config = dsl::cargo_configs
      .filter(dsl::key.eq(key))
      .get_result::<CargoConfigDbModel>(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpError>(config)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = serde_json::from_value::<CargoConfigPartial>(dbmodel.config)
    .map_err(|e| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to deserialize config: {e}"),
    })?;

  Ok(CargoConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: config.name,
    version: dbmodel.version,
    cargo_key: dbmodel.cargo_key,
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
  key: &str,
  pool: &Pool,
) -> Result<GenericDelete, HttpError> {
  use crate::schema::cargo_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::delete(dsl::cargo_configs)
      .filter(dsl::cargo_key.eq(key))
      .execute(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(GenericDelete { count: res })
}

pub async fn list_by_cargo(
  key: &str,
  pool: &Pool,
) -> Result<Vec<CargoConfig>, HttpError> {
  use crate::schema::cargo_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = dsl::cargo_configs
      .filter(dsl::cargo_key.eq(key))
      .get_results::<CargoConfigDbModel>(&mut conn)
      .map_err(db_error("cargo config"))?;
    Ok::<_, HttpError>(configs)
  })
  .await
  .map_err(db_blocking_error)?;

  let configs = dbmodels
    .into_iter()
    .map(|dbmodel| {
      let config = serde_json::from_value::<CargoConfigPartial>(dbmodel.config)
        .map_err(|e| HttpError {
          status: StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Failed to deserialize config: {e}"),
        })?;

      Ok(CargoConfig {
        key: dbmodel.key,
        created_at: dbmodel.created_at,
        name: config.name,
        version: dbmodel.version,
        cargo_key: dbmodel.cargo_key,
        replication: config.replication,
        container: config.container,
      })
    })
    .collect::<Result<Vec<CargoConfig>, HttpError>>()?;

  Ok(configs)
}
