use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::{generic, cargo_config};

use crate::{utils, models, schema};

/// ## Create
///
/// Create a cargo config item in database for given cargo
///
/// ## Arguments
///
/// - [cargo_key](str) - Cargo key
/// - [item](cargo_config::CargoConfigPartial) - Cargo config item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](cargo_config::CargoConfig) - The created cargo config
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  cargo_key: &str,
  item: &cargo_config::CargoConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> io_error::IoResult<cargo_config::CargoConfig> {
  let cargo_key = cargo_key.to_owned();
  let version = version.to_owned();
  let dbmodel = models::CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    data: serde_json::to_value(item.clone())
      .map_err(|e| e.map_err_context(|| "Invalid Config"))?,
    metadata: item.metadata.clone(),
  };

  let dbmodel = utils::repository::generic_insert_with_res::<
    _,
    _,
    models::CargoConfigDbModel,
  >(pool, dbmodel)
  .await?;

  let config = dbmodel.into_cargo_config(item);
  Ok(config)
}

/// ## Find by key
///
/// Find a cargo config item in database for given key
///
/// ## Arguments
///
/// - [key](uuid::Uuid) - Cargo config key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](cargo_config::CargoConfig) - The found cargo config
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &models::Pool,
) -> io_error::IoResult<cargo_config::CargoConfig> {
  let key = *key;

  let dbmodel: models::CargoConfigDbModel = utils::repository::generic_find_by_id::<
    schema::cargo_configs::table,
    _,
    _,
  >(pool, key)
  .await?;

  let config = serde_json::from_value::<cargo_config::CargoConfigPartial>(
    dbmodel.data.clone(),
  )
  .map_err(|err| err.map_err_context(|| "cargo_config::CargoConfigPartial"))?;

  Ok(dbmodel.into_cargo_config(&config))
}

/// ## Delete by cargo key
///
/// Delete all cargo config items in database for given cargo key
///
/// ## Arguments
///
/// - [key](str) - Cargo key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The number of deleted items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_cargo_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete::<schema::cargo_configs::table, _>(
    pool,
    schema::cargo_configs::dsl::cargo_key.eq(key),
  )
  .await
}

/// ## List by cargo key
///
/// List all cargo config items in database for given cargo key.
///
/// ## Arguments
///
/// - [key](str) - Cargo key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<cargo_config::CargoConfig>) - The list of cargo configs
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_by_cargo_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<cargo_config::CargoConfig>> {
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = schema::cargo_configs::dsl::cargo_configs
      .order(schema::cargo_configs::dsl::created_at.desc())
      .filter(schema::cargo_configs::dsl::cargo_key.eq(key))
      .get_results::<models::CargoConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "cargo_config::CargoConfig"))?;
    Ok::<_, io_error::IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel: models::CargoConfigDbModel| {
      let config = serde_json::from_value::<cargo_config::CargoConfigPartial>(
        dbmodel.data.clone(),
      )
      .map_err(|err| {
        err.map_err_context(|| "cargo_config::CargoConfigPartial")
      })?;
      Ok(dbmodel.into_cargo_config(&config))
    })
    .collect::<Result<Vec<cargo_config::CargoConfig>, io_error::IoError>>()?;
  Ok(configs)
}
