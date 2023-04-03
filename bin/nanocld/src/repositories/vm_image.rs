use ntex::web;
use diesel::prelude::*;

use crate::utils;
use crate::models::{Pool, VmImageDbModel, VmImageUpdateDbModel};
use crate::error::HttpError;
use crate::repositories::error::{db_error, db_blocking_error};

pub async fn create(
  item: &VmImageDbModel,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
  use crate::schema::vm_images::dsl;

  let item = item.clone();

  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::insert_into(dsl::vm_images)
      .values(&item)
      .get_result(&mut conn)
      .map_err(db_error("vm_image"))?;
    Ok::<_, HttpError>(item)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(item)
}

pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
  use crate::schema::vm_images::dsl;

  let name = name.to_owned();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::vm_images
      .filter(dsl::name.eq(&name))
      .get_result(&mut conn)
      .map_err(db_error(&format!("vm_image {name}")))?;
    Ok::<_, HttpError>(item)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(item)
}

pub async fn find_by_parent(
  parent: &str,
  pool: &Pool,
) -> Result<Vec<VmImageDbModel>, HttpError> {
  use crate::schema::vm_images::dsl;

  let parent = parent.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
      .filter(dsl::parent.eq(&parent))
      .load::<VmImageDbModel>(&mut conn)
      .map_err(db_error("vm_image"))?;
    Ok::<_, HttpError>(items)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(items)
}

pub async fn delete_by_name(name: &str, pool: &Pool) -> Result<(), HttpError> {
  use crate::schema::vm_images::dsl;

  let name = name.to_owned();
  let pool = pool.clone();
  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::delete(dsl::vm_images.filter(dsl::name.eq(name)))
      .execute(&mut conn)
      .map_err(db_error("vm_image"))?;
    Ok::<_, HttpError>(())
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(())
}

pub async fn list(pool: &Pool) -> Result<Vec<VmImageDbModel>, HttpError> {
  use crate::schema::vm_images::dsl;

  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
      .load::<VmImageDbModel>(&mut conn)
      .map_err(db_error("vm_image"))?;
    Ok::<_, HttpError>(items)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(items)
}

pub async fn update_by_name(
  name: &str,
  item: &VmImageUpdateDbModel,
  pool: &Pool,
) -> Result<VmImageDbModel, HttpError> {
  use crate::schema::vm_images::dsl;

  let name = name.to_owned();
  let item = item.clone();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::update(dsl::vm_images.filter(dsl::name.eq(name)))
      .set(item)
      .get_result(&mut conn)
      .map_err(db_error("vm_image"))?;
    Ok::<_, HttpError>(item)
  })
  .await
  .map_err(db_blocking_error)?;
  Ok(item)
}
