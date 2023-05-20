use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::resource::{Resource, ResourcePartial, ResourceQuery};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::{utils, repositories};
use crate::models::{
  Pool, ResourceDbModel, ResourceConfigDbModel, ResourceUpdateModel,
};

use super::resource_config;

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
pub async fn create(item: &ResourcePartial, pool: &Pool) -> IoResult<Resource> {
  use crate::schema::resources::dsl;

  let pool = pool.clone();
  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: item.name.to_owned(),
    version: item.version.to_owned(),
    data: item.config.clone(),
  };

  let config = resource_config::create(&config, &pool).await?;

  let new_item = ResourceDbModel {
    key: item.name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: item.kind.clone(),
    config_key: config.key.to_owned(),
  };

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::resources)
      .values(&new_item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Resource"))?;
    Ok::<_, IoError>(new_item)
  })
  .await?;

  let item = Resource {
    name: item.key,
    created_at: item.created_at,
    updated_at: config.created_at,
    kind: item.kind,
    version: config.version,
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
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::resources::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = diesel::delete(dsl::resources)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Resource"))?;
    Ok::<_, IoError>(count)
  })
  .await?;

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
pub async fn find(
  pool: &Pool,
  query: Option<ResourceQuery>,
) -> IoResult<Vec<Resource>> {
  use crate::schema::resources;
  use crate::schema::resource_configs;

  let pool = pool.clone();
  let res: Vec<(ResourceDbModel, ResourceConfigDbModel)> =
    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let req = match query {
        Some(qs) => {
          let mut req = resources::table
            .inner_join(resource_configs::table)
            .into_boxed();
          if let Some(kind) = &qs.kind {
            req = req.filter(resources::kind.eq(kind.to_string()));
          }
          if let Some(contains) = &qs.contains {
            let contains = serde_json::from_str::<serde_json::Value>(contains)
              .map_err(|err| err.map_err_context(|| "Contains"))?;
            req = req.filter(resource_configs::data.contains(contains));
          }

          req = req.order(resources::created_at.desc());

          req.load(&mut conn)
        }
        None => resources::table
          .order(resources::created_at.desc())
          .inner_join(resource_configs::table)
          .load(&mut conn),
      };

      let res = req.map_err(|err| err.map_err_context(|| "Resource"))?;

      Ok::<_, IoError>(res)
    })
    .await?;

  let items = res
    .into_iter()
    .map(|e| {
      let resource = e.0;
      let config = e.1;
      Ok::<_, IoError>(Resource {
        name: resource.key,
        created_at: resource.created_at,
        updated_at: config.created_at,
        kind: resource.kind,
        version: config.version,
        config_key: resource.config_key,
        config: config.data,
      })
    })
    .collect::<Result<Vec<Resource>, IoError>>()?;
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
pub async fn inspect_by_key(key: &str, pool: &Pool) -> IoResult<Resource> {
  use crate::schema::resources;
  use crate::schema::resource_configs;

  let key = key.to_owned();
  let pool = pool.clone();

  let res: (ResourceDbModel, ResourceConfigDbModel) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = resources::table
      .inner_join(resource_configs::table)
      .filter(resources::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Resource"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  let item = Resource {
    name: res.0.key,
    created_at: res.0.created_at,
    updated_at: res.1.created_at,
    kind: res.0.kind,
    version: res.1.version,
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
pub async fn patch(item: &ResourcePartial, pool: &Pool) -> IoResult<Resource> {
  use crate::schema::resources;

  let pool = pool.clone();
  let key = item.name.clone();
  let resource =
    repositories::resource::inspect_by_key(&item.name, &pool).await?;

  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: resource.name.to_owned(),
    version: item.version.clone(),
    data: item.config.clone(),
  };

  let config = resource_config::create(&config, &pool).await?;

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
      .map_err(|err| err.map_err_context(|| "Resource"))?;
    Ok::<_, IoError>(())
  })
  .await?;

  let item = Resource {
    name: resource.name,
    created_at: resource.created_at,
    updated_at: config.created_at,
    kind: resource.kind,
    version: config.version,
    config_key: config.key,
    config: config.data,
  };
  Ok(item)
}

pub async fn create_or_patch(
  resource: &ResourcePartial,
  pool: &Pool,
) -> IoResult<Resource> {
  if inspect_by_key(&resource.name, pool).await.is_ok() {
    return patch(resource, pool).await;
  }
  create(resource, pool).await
}
