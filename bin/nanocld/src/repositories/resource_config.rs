use nanocl_stubs::generic;
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::resource::ResourceConfig;

use crate::{utils, schema};
use crate::models;

/// ## Create
///
/// Create a resource config in database
///
/// ## Arguments
///
/// - [item](models::ResourceConfigDbModel) - Resource config item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::ResourceConfigDbModel) - Resource config created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &models::ResourceConfigDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::ResourceConfigDbModel> {
  let item = item.clone();

  utils::repository::generic_insert_with_res(pool, item).await
}

/// ## Delete by resource key
///
/// Delete all resource config by a resource key
///
/// ## Arguments
///
/// - [key](str) - Resource key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Resource config deleted
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_resource_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete::<schema::resource_configs::table, _>(
    pool,
    schema::resource_configs::dsl::resource_key.eq(key),
  )
  .await
}

/// ## List by resource key
///
/// List all resource config by resource key
///
/// ## Arguments
///
/// - [key](str) - Resource key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ResourceConfig>) - Resource config list
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_by_resource_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<ResourceConfig>> {
  use crate::schema::resource_configs::dsl;
  let key = key.to_owned();
  let pool = pool.clone();
  let models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::resource_configs
      .order(dsl::created_at.desc())
      .filter(dsl::resource_key.eq(key))
      .load::<models::ResourceConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceConfig"))?;
    Ok::<_, io_error::IoError>(items)
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
/// - [key](uuid::Uuid) - Resource config key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](ResourceConfig) - Resource config found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &models::Pool,
) -> io_error::IoResult<ResourceConfig> {
  use crate::schema::resource_configs::dsl;
  let key = *key;
  let pool = pool.clone();
  let model = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::resource_configs
      .filter(dsl::key.eq(key))
      .first::<models::ResourceConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceConfig"))?;
    Ok::<_, io_error::IoError>(item)
  })
  .await?;
  Ok(ResourceConfig::from(model))
}
