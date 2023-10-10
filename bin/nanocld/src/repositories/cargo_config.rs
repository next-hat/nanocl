use nanocl_macros_getters::{
  repository_create, repository_find_by_id, repository_delete_by_id,
  repository_delete_by,
};
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};

use crate::serializers::cargo_config::serialize_cargo_config;
use crate::utils;
use crate::models::{Pool, CargoConfigDbModel};

/// ## Create
///
/// Create a cargo config item in database for given cargo
///
/// ## Arguments
///
/// - [cargo_key](str) - Cargo key
/// - [item](CargoConfigPartial) - Cargo config item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoConfig) - The created cargo config
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  cargo_key: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<CargoConfig> {
  use crate::schema::cargo_configs::dsl;
  let cargo_key = cargo_key.to_owned();
  let version = version.to_owned();
  let dbmodel = CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    data: serde_json::to_value(item.clone())
      .map_err(|e| e.map_err_context(|| "Invalid Config"))?,
    metadata: item.metadata.clone(),
  };

  let dbmodel =
    repository_create!(dsl::cargo_configs, dbmodel, pool, "CargoConfig");

  // let pool = pool.clone();
  // let dbmodel: CargoConfigDbModel = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   diesel::insert_into(dsl::cargo_configs)
  //     .values(&dbmodel)
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
  //   Ok::<_, IoError>(dbmodel)
  // })
  // .await?;

  // let item = item.clone();

  let config = serialize_cargo_config(dbmodel, item);
  Ok(config)
}

/// ## Find by key
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
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> IoResult<CargoConfig> {
  use crate::schema::cargo_configs::dsl;
  let key = *key;

  let dbmodel: CargoConfigDbModel =
    repository_find_by_id!(dsl::cargo_configs, key, pool, "CargoConfig");
  // let pool = pool.clone();
  // let dbmodel = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let config = dsl::cargo_configs
  //     .filter(dsl::key.eq(key))
  //     .get_result::<CargoConfigDbModel>(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
  //   Ok::<_, IoError>(config)
  // })
  // .await?;

  let config =
    serde_json::from_value::<CargoConfigPartial>(dbmodel.data.clone())
      .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;

  Ok(serialize_cargo_config(dbmodel, &config))
}

/// ## Delete by cargo key
///
/// Delete all cargo config items in database for given cargo key
///
/// ## Arguments
///
/// - [key](str) - Cargo key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The number of deleted items
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::cargo_configs::dsl;
  // let key = key.to_owned();
  // let pool = pool.clone();
  // let res = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let res = diesel::delete(dsl::cargo_configs)
  //     .filter(dsl::cargo_key.eq(key))
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
  //   Ok::<_, IoError>(res)
  // })
  // .await?;
  let res = repository_delete_by!(
    dsl::cargo_configs,
    dsl::cargo_key,
    key,
    pool,
    "CargoConfig"
  );
  Ok(GenericDelete { count: res })
}

/// ## List by cargo key
///
/// List all cargo config items in database for given cargo key.
///
/// ## Arguments
///
/// - [key](str) - Cargo key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<CargoConfig>) - The list of cargo configs
///   - [Err](IoError) - Error during the operation
///
pub async fn list_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<Vec<CargoConfig>> {
  use crate::schema::cargo_configs::dsl;
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = dsl::cargo_configs
      .order(dsl::created_at.desc())
      .filter(dsl::cargo_key.eq(key))
      .get_results::<CargoConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
    Ok::<_, IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel| {
      let config =
        serde_json::from_value::<CargoConfigPartial>(dbmodel.data.clone())
          .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;
      Ok(serialize_cargo_config(dbmodel, &config))
    })
    .collect::<Result<Vec<CargoConfig>, IoError>>()?;
  Ok(configs)
}
