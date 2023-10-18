use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::{generic, resource};

use crate::{utils, models, repositories, schema};

use super::resource_config;

/// ## Create
///
/// Create a resource item in database from a `resource::ResourcePartial`
/// and return a `resource::Resource` with the generated key.
///
/// ## Arguments
///
/// - [item](resource::ResourcePartial) - resource::Resource item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](resource::Resource) - resource::Resource created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &resource::ResourcePartial,
  pool: &models::Pool,
) -> io_error::IoResult<resource::Resource> {
  let pool = pool.clone();
  let config = models::ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: item.name.to_owned(),
    version: item.version.to_owned(),
    data: item.data.clone(),
    metadata: item.metadata.clone(),
  };
  let config = resource_config::create(&config, &pool).await?;
  let new_item = models::ResourceDbModel {
    key: item.name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: item.kind.clone(),
    config_key: config.key.to_owned(),
  };

  let dbmodel: models::ResourceDbModel =
    utils::repository::generic_insert_with_res(&pool, new_item).await?;

  let item = dbmodel.into_resource(config);
  Ok(item)
}

/// ## Delete by key
///
/// Delete a resource item from database by key
///
/// ## Arguments
///
/// - [key](str) - resource::Resource key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericDelete) - Number of deleted items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete_by_id::<schema::resources::table, _>(
    pool, key,
  )
  .await
}

/// ## Find
///
/// Find resources from database for given query
///
/// ## Arguments
///
/// - [query](resource::ResourceQuery) - Query to filter resources
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<resource::Resource>) - List of resources
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find(
  query: Option<resource::ResourceQuery>,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<resource::Resource>> {
  use crate::schema::resources;
  use crate::schema::resource_configs;
  let pool = pool.clone();
  let res: Vec<(models::ResourceDbModel, models::ResourceConfigDbModel)> =
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
      let res =
        req.map_err(|err| err.map_err_context(|| "resource::Resource"))?;
      Ok::<_, io_error::IoError>(res)
    })
    .await?;
  let items = res
    .into_iter()
    .map(|e| {
      let resource = e.0;
      let config = e.1;
      Ok::<_, io_error::IoError>(resource.into_resource(config))
    })
    .collect::<Result<Vec<resource::Resource>, io_error::IoError>>()?;
  Ok(items)
}

/// ## Inspect by key
///
/// Inspect a resource item in database by his key
///
/// ## Arguments
///
/// - [key](str) - resource::Resource key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](resource::Resource) - resource::Resource item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn inspect_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<resource::Resource> {
  use crate::schema::resources;
  use crate::schema::resource_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let res: (models::ResourceDbModel, models::ResourceConfigDbModel) =
    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = resources::table
        .inner_join(resource_configs::table)
        .filter(resources::key.eq(key))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "resource::Resource"))?;
      Ok::<_, io_error::IoError>(res)
    })
    .await?;
  let item = res.0.into_resource(res.1);
  Ok(item)
}

/// ## Put
///
/// Set given `resource::ResourcePartial` as the current config for the resource
/// and return a `resource::Resource` with the new config
///
/// ## Arguments
///
/// - [item](resource::ResourcePartial) - resource::Resource item to put
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](resource::Resource) - resource::Resource item
///   - [Err](io_error::IoError) - Error during the operation
///
/// //TODO: Normalize names
pub async fn put(
  item: &resource::ResourcePartial,
  pool: &models::Pool,
) -> io_error::IoResult<resource::Resource> {
  let key = item.name.clone();
  let resource =
    repositories::resource::inspect_by_key(&item.name, pool).await?;
  let config = models::ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: resource.name.to_owned(),
    version: item.version.clone(),
    data: item.data.clone(),
    metadata: item.metadata.clone(),
  };
  let config = resource_config::create(&config, &pool).await?;
  let resource_update = models::ResourceUpdateModel {
    key: None,
    config_key: Some(config.key.to_owned()),
  };

  let dbmodel = utils::repository::generic_update_by_id_with_res::<
    schema::resources::table,
    _,
    _,
    models::ResourceDbModel,
  >(pool, key, resource_update)
  .await?;

  let item = dbmodel.into_resource(config);
  Ok(item)
}
