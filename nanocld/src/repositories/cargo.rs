use ntex::web;
use diesel::prelude::*;

use nanocl_models::cargo::Cargo;
use nanocl_models::generic::GenericDelete;

use crate::schema::cargoes;
use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{
  Pool, CargoPartial, CargoDbModel, NamespaceDbModel, CargoUpdateDbModel,
};

use super::cargo_config;
use super::error::db_blocking_error;

/// ## Find cargo items by namespace
///
/// ## Arguments
/// - [nsp](NamespaceItem) - Namespace item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<CargoDbModel>) - List a cargo found
///  - [Err](HttpResponseError) - Error during the operation
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

pub async fn count_by_namespace(
  namespace: String,
  pool: &Pool,
) -> Result<i64, HttpResponseError> {
  use crate::schema::cargoes::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let count = web::block(move || {
    cargoes::table
      .filter(dsl::namespace_name.eq(namespace))
      .count()
      .get_result(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(count)
}
