use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::resource::ResourceConfig;

use crate::utils;
use crate::models::{Pool, ResourceConfigDbModel};

/// ## Create
///
/// Create a resource config in database
///
/// ## Arguments
///
/// * [item](ResourceConfigDbModel) - Resource config item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceConfigDbModel) - Resource config created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &ResourceConfigDbModel,
  pool: &Pool,
) -> IoResult<ResourceConfigDbModel> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}

/// ## Delete by resource key
///
/// Delete all resource config by a resource key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Resource config deleted
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_resource_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::resource_configs;
  let key = key.to_owned();
  super::generic::delete::<resource_configs::table, _>(
    resource_configs::dsl::resource_key.eq(key),
    pool,
  )
  .await
}

/// ## List by resource key
///
/// List all resource config by resource key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<ResourceConfig>) - Resource config list
///   * [Err](IoError) - Error during the operation
///
pub async fn list_by_resource_key(
  key: &str,
  pool: &Pool,
) -> IoResult<Vec<ResourceConfig>> {
  use crate::schema::resource_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = resource_configs::dsl::resource_configs
      .order(resource_configs::dsl::created_at.desc())
      .filter(resource_configs::dsl::resource_key.eq(key))
      .load::<ResourceConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceConfig"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  let models = models
    .into_iter()
    .map(ResourceConfig::from)
    .collect::<Vec<_>>();
  Ok(models)
}

/// ## Find by key
///
/// Find resource config by key
///
/// ## Arguments
///
/// * [key](uuid::Uuid) - Resource config key
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceConfig) - Resource config found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> IoResult<ResourceConfig> {
  use crate::schema::resource_configs;
  let key = *key;
  let pool = pool.clone();
  let model = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = resource_configs::dsl::resource_configs
      .filter(resource_configs::dsl::key.eq(key))
      .first::<ResourceConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceConfig"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(ResourceConfig::from(model))
}
