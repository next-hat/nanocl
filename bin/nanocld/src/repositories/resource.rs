use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::resource::{Resource, ResourcePartial};

use crate::repositories::error::db_error;
use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::{
  Pool, ResourceDbModel, ResourceConfigDbModel, ResourceUpdateModel,
};

use super::resource_config;
use super::error::db_blocking_error;

/// ## Create resource
///
/// Create a resource item in database
///
/// ## Arguments
///
/// - [item](ResourcePartial) - Resource item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Resource) - Resource created
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::resource::ResourcePartial;
/// use crate::repositories;
///
/// let item = ResourcePartial {
///  name: String::from("my-resource"),
///  // fill your values
/// };
///
/// let item = repositories::resource::create(item, &pool).await;
/// ```
///
pub async fn create(
  item: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  use crate::schema::resources::dsl;

  let pool = pool.to_owned();
  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    resource_key: item.name.to_owned(),
    data: item.config,
  };

  let config = resource_config::create(config.to_owned(), &pool).await?;

  let new_item = ResourceDbModel {
    key: item.name.to_owned(),
    kind: item.kind.into(),
    config_key: config.key.to_owned(),
  };

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::resources)
      .values(&new_item)
      .execute(&mut conn)
      .map_err(db_error("resource"))?;
    Ok::<_, HttpResponseError>(new_item)
  })
  .await
  .map_err(db_blocking_error)?;

  let item = Resource {
    name: item.key,
    kind: item.kind.into(),
    config_key: config.key,
    config: config.data,
  };

  Ok(item)
}

/// ## Delete resource by key
///
/// Delete a resource item in database by key
///
/// ## Arguments
///
/// - [key](String) - Resource key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - Number of deleted items
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// repositories::resource::delete_by_key(String::from("my-resource"), &pool).await;
/// ```
///
pub async fn delete_by_key(
  key: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::resources::dsl;

  let pool = pool.to_owned();
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = diesel::delete(dsl::resources)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
      .map_err(db_error("resource"))?;
    Ok::<_, HttpResponseError>(count)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(GenericDelete { count })
}

/// ## Find resources
///
/// Find all resources in database
///
/// ## Arguments
///
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<Resource>) - List of resources
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let items = repositories::resource::find(&pool).await;
/// ```
///
pub async fn find(pool: &Pool) -> Result<Vec<Resource>, HttpResponseError> {
  use crate::schema::resources;

  let pool = pool.to_owned();
  let res: Vec<(ResourceDbModel, ResourceConfigDbModel)> =
    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = resources::table
        .inner_join(crate::schema::resource_configs::table)
        .load(&mut conn)
        .map_err(db_error("resource"))?;
      Ok::<_, HttpResponseError>(res)
    })
    .await
    .map_err(db_blocking_error)?;

  let items = res
    .into_iter()
    .map(|e| Resource {
      name: e.0.key,
      kind: e.0.kind.into(),
      config_key: e.0.config_key,
      config: e.1.data,
    })
    .collect::<Vec<Resource>>();
  Ok(items)
}

/// ## Inspect resource by key
///
/// Inspect a resource item in database by key
///
/// ## Arguments
///
/// - [key](String) - Resource key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Resource) - Resource item
/// - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let item = repositories::resource::inspect_by_key(String::from("my-resource"), &pool).await;
/// ```
///
pub async fn inspect_by_key(
  key: String,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  use crate::schema::resources;
  use crate::schema::resource_configs;

  let pool = pool.to_owned();
  let res: (ResourceDbModel, ResourceConfigDbModel) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = resources::table
      .inner_join(resource_configs::table)
      .filter(resources::key.eq(key))
      .get_result(&mut conn)
      .map_err(db_error("resource"))?;
    Ok::<_, HttpResponseError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  let item = Resource {
    name: res.0.key,
    kind: res.0.kind.into(),
    config_key: res.0.config_key,
    config: res.1.data,
  };

  Ok(item)
}

/// ## Update resource by key
///
/// Update a resource item in database by key
///
/// ## Arguments
///
/// - [key](String) - Resource key
/// - [item](serde_json::Value) - Resource item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Resource) - Resource item
/// - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let item = repositories::resource::update_by_id(String::from("my-resource"), json!({"foo": "bar"}), &pool).await;
/// ```
///
pub async fn update_by_id(
  key: String,
  item: serde_json::Value,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  use crate::schema::resources;

  let pool = pool.to_owned();
  let resource =
    repositories::resource::inspect_by_key(key.to_owned(), &pool).await?;

  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    resource_key: key.to_owned(),
    data: item,
  };

  let config = resource_config::create(config.to_owned(), &pool).await?;

  let resource_update = ResourceUpdateModel {
    key: None,
    config_key: Some(config.key.to_owned()),
  };

  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::update(resources::table)
      .filter(resources::key.eq(key))
      .set(&resource_update)
      .execute(&mut conn)
      .map_err(db_error("resource"))?;
    Ok::<_, HttpResponseError>(())
  })
  .await
  .map_err(db_blocking_error)?;

  let item = Resource {
    name: resource.name,
    kind: resource.kind,
    config_key: config.key,
    config: config.data,
  };
  Ok(item)
}
