use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::resource::ResourceConfig;

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, ResourceConfigDbModel};

use super::error::{db_error, db_blocking_error};

/// ## Create resource config
///
/// Create a resource config in database
///
/// ## Arguments
///
/// - [item](ResourceConfigDbModel) - Resource config item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](ResourceConfigDbModel) - Resource config created
/// - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::resource::ResourceConfigDbModel;
/// use crate::repositories;
///
/// let item = ResourceConfigDbModel {
///   // Fill data
/// };
/// let resource_config = repositories::resource_config::create(item, &pool).await;
/// ```
///
pub async fn create(
  item: &ResourceConfigDbModel,
  pool: &Pool,
) -> Result<ResourceConfigDbModel, HttpError> {
  use crate::schema::resource_configs::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::resource_configs)
      .values(&item)
      .execute(&mut conn)
      .map_err(db_error("resource config"))?;
    Ok::<_, HttpError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(dbmodel)
}

/// ## Delete resource config by resource key
///
/// Delete all resource config by resource key
///
/// ## Arguments
///
/// - [key](&str) - Resource key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Resource config deleted
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// repositories::resource_config::delete_by_resource_key(&str::from("my-resource"), &pool).await;
/// ```
///
pub async fn delete_by_resource_key(
  key: &str,
  pool: &Pool,
) -> Result<(), HttpError> {
  use crate::schema::resource_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::delete(dsl::resource_configs.filter(dsl::resource_key.eq(key)))
      .execute(&mut conn)
      .map_err(db_error("resource config"))?;
    Ok::<_, HttpError>(())
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(())
}

pub async fn list_by_resource(
  key: &str,
  pool: &Pool,
) -> Result<Vec<ResourceConfig>, HttpError> {
  use crate::schema::resource_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::resource_configs
      .filter(dsl::resource_key.eq(key))
      .load::<ResourceConfigDbModel>(&mut conn)
      .map_err(db_error("resource config"))?;
    Ok::<_, HttpError>(items)
  })
  .await
  .map_err(db_blocking_error)?;

  let models = models
    .into_iter()
    .map(ResourceConfig::from)
    .collect::<Vec<_>>();

  Ok(models)
}

pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> Result<ResourceConfig, HttpError> {
  use crate::schema::resource_configs::dsl;

  let key = *key;
  let pool = pool.clone();

  let model = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::resource_configs
      .filter(dsl::key.eq(key))
      .first::<ResourceConfigDbModel>(&mut conn)
      .map_err(db_error("resource config"))?;
    Ok::<_, HttpError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(ResourceConfig::from(model))
}
