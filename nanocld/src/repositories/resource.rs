use ntex::web;
use diesel::prelude::*;

use nanocl_models::generic::GenericDelete;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{
  Pool, Resource, ResourcePartial, ResourceDbModel, ResourceConfigDbModel,
};

use super::resource_config;
use super::error::db_blocking_error;

pub async fn create(
  item: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  use crate::schema::resources::dsl;

  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    resource_key: item.name.to_owned(),
    data: item.config,
  };

  let config = resource_config::create(config.to_owned(), pool).await?;

  let new_item = ResourceDbModel {
    key: item.name.to_owned(),
    kind: item.kind,
    config_key: config.key.to_owned(),
  };

  let mut conn = utils::store::get_pool_conn(pool)?;
  let item = web::block(move || {
    diesel::insert_into(dsl::resources)
      .values(&new_item)
      .execute(&mut conn)?;
    Ok(new_item)
  })
  .await
  .map_err(db_blocking_error)?;

  let item = Resource {
    name: item.key,
    kind: item.kind,
    config_key: config.key,
    config: config.data,
  };

  Ok(item)
}

pub async fn delete_by_key(
  key: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::resources::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    diesel::delete(dsl::resources)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(GenericDelete { count: res })
}

pub async fn find(pool: &Pool) -> Result<Vec<Resource>, HttpResponseError> {
  use crate::schema::resources;

  let mut conn = utils::store::get_pool_conn(pool)?;

  let res: Vec<(ResourceDbModel, ResourceConfigDbModel)> =
    web::block(move || {
      resources::table
        .inner_join(crate::schema::resource_configs::table)
        .load(&mut conn)
    })
    .await
    .map_err(db_blocking_error)?;

  let items = res
    .into_iter()
    .map(|e| Resource {
      name: e.0.key,
      kind: e.0.kind,
      config_key: e.0.config_key,
      config: e.1.data,
    })
    .collect::<Vec<Resource>>();
  Ok(items)
}
