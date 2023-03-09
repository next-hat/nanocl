use ntex::web;
use diesel::prelude::*;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{
  Pool, ResourceKindPartial, ResourceKindDbModel, ResourceKindVersionDbModel,
};

use super::error::{db_error, db_blocking_error};

pub async fn create_version(
  item: ResourceKindPartial,
  pool: &Pool,
) -> Result<ResourceKindVersionDbModel, HttpResponseError> {
  use crate::schema::resource_kind_versions::dsl;

  let kind_version = ResourceKindVersionDbModel {
    resource_kind_name: item.name.clone(),
    version: item.version.clone(),
    schema: item.schema,
    created_at: chrono::Utc::now().naive_utc(),
  };
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::resource_kind_versions)
      .values(&kind_version)
      .execute(&mut conn)
      .map_err(db_error("resource kind version"))?;
    Ok::<_, HttpResponseError>(kind_version)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn get_version(
  name: &str,
  version: &str,
  pool: &Pool,
) -> Result<ResourceKindVersionDbModel, HttpResponseError> {
  use crate::schema::resource_kind_versions::dsl;

  let pool = pool.clone();
  let name = name.to_owned();
  let version = version.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    dsl::resource_kind_versions
      .filter(dsl::resource_kind_name.eq(name))
      .filter(dsl::version.eq(version))
      .get_result(&mut conn)
      .map_err(db_error("resource kind version"))
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> Result<ResourceKindDbModel, HttpResponseError> {
  use crate::schema::resource_kinds;

  let pool = pool.clone();
  let name = name.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    resource_kinds::dsl::resource_kinds
      .filter(resource_kinds::dsl::name.eq(name))
      .get_result::<ResourceKindDbModel>(&mut conn)
      .map_err(db_error("resource kind"))
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn create(
  item: ResourceKindPartial,
  pool: &Pool,
) -> Result<ResourceKindDbModel, HttpResponseError> {
  use crate::schema::resource_kinds::dsl;

  let kind = ResourceKindDbModel {
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::resource_kinds)
      .values(&kind)
      .execute(&mut conn)
      .map_err(db_error("resource kind"))?;
    Ok::<_, HttpResponseError>(kind)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}
