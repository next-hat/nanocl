use ntex::web;
use diesel::prelude::*;

use crate::repositories::error::db_blocking_error;
use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{Pool, ResourceConfigDbModel, ResourceUpdateModel};

pub async fn create(
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

pub async fn delete_by_resource_key(
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
