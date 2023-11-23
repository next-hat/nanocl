use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::utils;
use crate::models::{Pool, CargoSpecDb};

/// ## Create
///
/// Create a cargo config item in database for given cargo
///
/// ## Arguments
///
/// * [cargo_key](str) - Cargo key
/// * [item](CargoSpecPartial) - Cargo config item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [CargoSpec](CargoSpec)
///
pub(crate) async fn create(
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
  let dbmodel = CargoSpecDb {
    key: uuid::Uuid::new_v4(),
    cargo_key,
    version,
    created_at: chrono::Utc::now().naive_utc(),
    data,
    metadata: item.metadata.clone(),
  };
  let dbmodel =
    super::generic::insert_with_res::<_, _, CargoSpecDb>(dbmodel, pool).await?;
  let config = dbmodel.into_cargo_spec(item);
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
/// [IoResult](IoResult) containing a [CargoSpec](CargoSpec)
///
pub(crate) async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> IoResult<CargoSpec> {
  use crate::schema::cargo_specs;
  let key = *key;
  let dbmodel: CargoSpecDb =
    super::generic::find_by_id::<cargo_specs::table, _, _>(key, pool).await?;
  let config = serde_json::from_value::<CargoSpecPartial>(dbmodel.data.clone())
    .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
  Ok(dbmodel.into_cargo_spec(&config))
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
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [CargoSpec](CargoSpec)
///
pub(crate) async fn list_by_cargo_key(
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
      .get_results::<CargoSpecDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "CargoSpec"))?;
    Ok::<_, IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel: CargoSpecDb| {
      let config =
        serde_json::from_value::<CargoSpecPartial>(dbmodel.data.clone())
          .map_err(|err| err.map_err_context(|| "CargoSpecPartial"))?;
      Ok(dbmodel.into_cargo_spec(&config))
    })
    .collect::<Result<Vec<CargoSpec>, IoError>>()?;
  Ok(configs)
}
