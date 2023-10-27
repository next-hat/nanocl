use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::resource::{Resource, ResourcePartial, ResourceQuery};

use crate::{utils, repositories};
use crate::models::{
  Pool, ResourceDbModel, ResourceUpdateModel, ResourceConfigDbModel,
};

/// ## Create
///
/// Create a resource item in database from a `ResourcePartial`
/// and return a `Resource` with the generated key.
///
/// ## Arguments
///
/// * [item](ResourcePartial) - Resource item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Resource) - Resource created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(item: &ResourcePartial, pool: &Pool) -> IoResult<Resource> {
  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: item.name.to_owned(),
    version: item.version.to_owned(),
    data: item.data.clone(),
    metadata: item.metadata.clone(),
  };
  let config = repositories::resource_config::create(&config, pool).await?;
  let new_item = ResourceDbModel {
    key: item.name.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: item.kind.clone(),
    config_key: config.key.to_owned(),
  };
  let dbmodel: ResourceDbModel =
    super::generic::insert_with_res(new_item, pool).await?;
  let item = dbmodel.into_resource(config);
  Ok(item)
}

/// ## Delete by key
///
/// Delete a resource item from database by key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Number of deleted items
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::resources;
  let key = key.to_owned();
  super::generic::delete_by_id::<resources::table, _>(key, pool).await
}

/// ## Find
///
/// Find resources from database for given query
///
/// ## Arguments
///
/// * [query](ResourceQuery) - Query to filter resources
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<Resource>) - List of resources
///   * [Err](IoError) - Error during the operation
///
pub async fn find(
  query: Option<ResourceQuery>,
  pool: &Pool,
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
          if let Some(exists) = &qs.exists {
            req = req.filter(resource_configs::data.has_key(exists));
          }
          if let Some(contains) = &qs.contains {
            let contains = serde_json::from_str::<serde_json::Value>(contains)
              .map_err(|err| err.map_err_context(|| "Contains"))?;
            req = req.filter(resource_configs::data.contains(contains));
          }
          if let Some(meta_exists) = &qs.meta_exists {
            req = req.filter(resource_configs::metadata.has_key(meta_exists));
          }
          if let Some(meta_contains) = &qs.meta_contains {
            let meta_contains =
              serde_json::from_str::<serde_json::Value>(meta_contains)
                .map_err(|err| err.map_err_context(|| "Meta contains"))?;
            req =
              req.filter(resource_configs::metadata.contains(meta_contains));
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
      Ok::<_, IoError>(resource.into_resource(config))
    })
    .collect::<Result<Vec<Resource>, IoError>>()?;
  Ok(items)
}

/// ## Inspect by key
///
/// Inspect a resource item in database by his key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Resource) - Resource item
///   * [Err](IoError) - Error during the operation
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
  let item = res.0.into_resource(res.1);
  Ok(item)
}

/// ## Put
///
/// Set given `ResourcePartial` as the current config for the resource
/// and return a `Resource` with the new config
///
/// ## Arguments
///
/// * [item](ResourcePartial) - Resource item to put
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Resource) - Resource item
///   * [Err](IoError) - Error during the operation
///
///
pub async fn put(item: &ResourcePartial, pool: &Pool) -> IoResult<Resource> {
  use crate::schema::resources;
  let key = item.name.clone();
  let resource =
    repositories::resource::inspect_by_key(&item.name, pool).await?;
  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    created_at: chrono::Utc::now().naive_utc(),
    resource_key: resource.name.to_owned(),
    version: item.version.clone(),
    data: item.data.clone(),
    metadata: item.metadata.clone(),
  };
  let config = repositories::resource_config::create(&config, pool).await?;
  let resource_update = ResourceUpdateModel {
    key: None,
    config_key: Some(config.key.to_owned()),
  };
  let dbmodel = super::generic::update_by_id_with_res::<
    resources::table,
    _,
    _,
    ResourceDbModel,
  >(key, resource_update, pool)
  .await?;
  let item = dbmodel.into_resource(config);
  Ok(item)
}
