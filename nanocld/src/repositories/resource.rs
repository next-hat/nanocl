use nanocl_models::generic::GenericDelete;
use ntex::web;
use diesel::prelude::*;

use crate::repositories::error::db_blocking_error;
use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{
  Pool, ResourceDbModel, ResourceConfigDbModel, ResourcePartial, ResourceKind,
  Resource,
};

pub async fn create_config(
  item: ResourceConfigDbModel,
  pool: &Pool,
) -> Result<ResourceConfigDbModel, HttpResponseError> {
  use crate::schema::resource_configs::dsl;
  let mut conn = utils::store::get_pool_conn(pool)?;
  let dbmodel = web::block(move || {
    diesel::insert_into(dsl::resource_configs)
      .values(&item)
      .execute(&mut conn)?;
    Ok(item)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(dbmodel)
}

pub async fn create(
  item: ResourcePartial,
  pool: &Pool,
) -> Result<Resource, HttpResponseError> {
  use crate::schema::resources::dsl;

  let config = ResourceConfigDbModel {
    key: uuid::Uuid::new_v4(),
    resource_key: item.name.to_owned(),
    config: item.config,
  };

  let config = create_config(config.to_owned(), pool).await?;

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
    config: config.config,
  };

  Ok(item)
}

pub async fn delete_config_by_resource_key(
  key: String,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  use crate::schema::resource_configs::dsl;
  let mut conn = utils::store::get_pool_conn(pool)?;
  web::block(move || {
    diesel::delete(dsl::resource_configs.filter(dsl::resource_key.eq(key)))
      .execute(&mut conn)?;
    Ok(())
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(())
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

