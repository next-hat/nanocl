use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_config::{CargoConfig, CargoConfigPartial};

use crate::utils;
use crate::models::{Pool, CargoConfigDbModel};

/// ## Create
///
/// Create a cargo config item in database for given cargo
///
/// ## Arguments
///
/// * [cargo_key](str) - Cargo key
/// * [item](CargoConfigPartial) - Cargo config item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [CargoConfig](CargoConfig)
///
pub(crate) async fn create(
  cargo_key: &str,
  item: &CargoConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<CargoConfig> {
  let cargo_key = cargo_key.to_owned();
  let version = version.to_owned();
  let mut data = serde_json::to_value(item.to_owned())
    .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;
  if let Some(meta) = data.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = CargoConfigDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    data,
    metadata: item.metadata.clone(),
  };
  let dbmodel =
    super::generic::insert_with_res::<_, _, CargoConfigDbModel>(dbmodel, pool)
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
/// * [key](uuid::Uuid) - Cargo config key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [CargoConfig](CargoConfig)
///
pub(crate) async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> IoResult<CargoConfig> {
  use crate::schema::cargo_configs;
  let key = *key;
  let dbmodel: CargoConfigDbModel =
    super::generic::find_by_id::<cargo_configs::table, _, _>(key, pool).await?;
  let config =
    serde_json::from_value::<CargoConfigPartial>(dbmodel.data.clone())
      .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;
  Ok(dbmodel.into_cargo_config(&config))
}

/// ## Delete by cargo key
///
/// Delete all cargo config items in database for given cargo key
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
pub(crate) async fn delete_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::cargo_configs;
  let key = key.to_owned();
  super::generic::delete::<cargo_configs::table, _>(
    cargo_configs::dsl::cargo_key.eq(key),
    pool,
  )
  .await
}

/// ## List by cargo key
///
/// List all cargo config items in database for given cargo key.
///
/// ## Arguments
///
/// * [key](str) - Cargo key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [CargoConfig](CargoConfig)
///
pub(crate) async fn list_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<Vec<CargoConfig>> {
  use crate::schema::cargo_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = cargo_configs::dsl::cargo_configs
      .order(cargo_configs::dsl::created_at.desc())
      .filter(cargo_configs::dsl::cargo_key.eq(key))
      .get_results::<CargoConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
    Ok::<_, IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel: CargoConfigDbModel| {
      let config =
        serde_json::from_value::<CargoConfigPartial>(dbmodel.data.clone())
          .map_err(|err| err.map_err_context(|| "CargoConfigPartial"))?;
      Ok(dbmodel.into_cargo_config(&config))
    })
    .collect::<Result<Vec<CargoConfig>, IoError>>()?;
  Ok(configs)
}
