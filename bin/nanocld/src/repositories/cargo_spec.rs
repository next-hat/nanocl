use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::utils;
use crate::models::{Pool, CargoSpecDbModel};

/// ## Create
///
/// Create a cargo spec item in database for given cargo
///
/// ## Arguments
///
/// * [cargo_key](str) - Cargo key
/// * [item](CargoSpecPartial) - Cargo spec item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](CargoConfig) - The created cargo spec
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  cargo_key: &str,
  item: &CargoSpecPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<CargoSpec> {
  let cargo_key = cargo_key.to_owned();
  let version = version.to_owned();
  let mut data = serde_json::to_value(item.to_owned())
    .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
  if let Some(meta) = data.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = CargoSpecDbModel {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    data,
    metadata: item.metadata.clone(),
  };
  let dbmodel =
    super::generic::insert_with_res::<_, _, CargoSpecDbModel>(dbmodel, pool)
      .await?;
  let config = dbmodel.to_spec_with_partial(item);
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](CargoConfig) - The found cargo config
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_key(key: &uuid::Uuid, pool: &Pool) -> IoResult<CargoSpec> {
  use crate::schema::cargo_specs;
  let key = *key;
  let dbmodel: CargoSpecDbModel =
    super::generic::find_by_id::<cargo_specs::table, _, _>(key, pool).await?;
  Ok(dbmodel.dezerialize_to_spec()?)
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - The number of deleted items
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::cargo_specs;
  let key = key.to_owned();
  super::generic::delete::<cargo_specs::table, _>(
    cargo_specs::dsl::cargo_key.eq(key),
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<CargoConfig>) - The list of cargo configs
///   * [Err](IoError) - Error during the operation
///
pub async fn list_by_cargo_key(
  key: &str,
  pool: &Pool,
) -> IoResult<Vec<CargoSpec>> {
  use crate::schema::cargo_specs;
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = cargo_specs::dsl::cargo_specs
      .order(cargo_specs::dsl::created_at.desc())
      .filter(cargo_specs::dsl::cargo_key.eq(key))
      .get_results::<CargoSpecDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "CargoConfig"))?;
    Ok::<_, IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel: CargoSpecDbModel| Ok(dbmodel.dezerialize_to_spec()?))
    .collect::<Result<Vec<CargoSpec>, IoError>>()?;
  Ok(configs)
}
