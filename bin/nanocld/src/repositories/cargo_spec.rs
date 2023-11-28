use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::cargo_spec::{CargoSpec, CargoSpecPartial};

use crate::utils;
use crate::models::{Pool, CargoSpecDb, FromSpec};

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
  let db_model = CargoSpecDb::try_from_spec_partial(cargo_key, version, item)?;
  let db_model =
    super::generic::insert_with_res::<_, _, CargoSpecDb>(db_model, pool)
      .await?;
  Ok(db_model.to_spec(item))
}

/// ## Find by key
///
/// Find a cargo spec item in database for given key
///
/// ## Arguments
///
/// * [key](uuid::Uuid) - Cargo spec key
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
  let db_model: CargoSpecDb =
    super::generic::find_by_id::<cargo_specs::table, _, _>(key, pool).await?;
  db_model.try_to_spec()
}

/// ## Delete by cargo key
///
/// Delete all cargo spec items in database for given cargo key
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
/// List all cargo spec items in database for given cargo key.
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
  let pool = Arc::clone(pool);
  let db_models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let db_models = cargo_specs::dsl::cargo_specs
      .order(cargo_specs::dsl::created_at.desc())
      .filter(cargo_specs::dsl::cargo_key.eq(key))
      .get_results::<CargoSpecDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "CargoSpec"))?;
    Ok::<_, IoError>(db_models)
  })
  .await?;
  let items = db_models
    .into_iter()
    .map(|dbmodel: CargoSpecDb| dbmodel.try_to_spec())
    .collect::<Result<Vec<CargoSpec>, IoError>>()?;
  Ok(items)
}
