use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::{Pool, VmImageDb, VmImageUpdateDb};

/// ## Create
///
/// Create a vm image in database for given `VmImageDb`
///
/// ## Arguments
///
/// * [item](VmImageDb) - Vm image item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [VmImageDb](VmImageDb)
///
pub(crate) async fn create(
  item: &VmImageDb,
  pool: &Pool,
) -> IoResult<VmImageDb> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}

/// ## Find by name
///
/// Find a vm image by his name in database and return a `VmImageDb`
///
/// ## Arguments
///
/// * [name](str) - Vm image name
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [VmImageDb](VmImageDb)
///
pub(crate) async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<VmImageDb> {
  use crate::schema::vm_images;
  let name = name.to_owned();
  super::generic::find_by_id::<vm_images::table, _, _>(name, pool).await
}

/// ## Find by parent
///
/// Find all vm images in database by his parent and return a `Vec<VmImageDb>`
///
/// ## Arguments
///
/// * [parent](str) - Vm image parent
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [VmImageDb](VmImageDb)
///
pub(crate) async fn find_by_parent(
  parent: &str,
  pool: &Pool,
) -> IoResult<Vec<VmImageDb>> {
  use crate::schema::vm_images;
  let parent = parent.to_owned();
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = vm_images::dsl::vm_images
      .filter(vm_images::dsl::parent.eq(&parent))
      .load::<VmImageDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Delete by name
///
/// Delete a vm image from database by his name
///
/// ## Arguments
///
/// * [name](str) - Vm image name
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::vm_images;
  let name = name.to_owned();
  super::generic::delete_by_id::<vm_images::table, _>(name, pool).await
}

/// ## List
///
/// List all vm images in database and return a `Vec<VmImageDb>`
///
/// ## Arguments
///
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [VmImageDb](VmImageDb)
///
pub(crate) async fn list(pool: &Pool) -> IoResult<Vec<VmImageDb>> {
  use crate::schema::vm_images;
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = vm_images::dsl::vm_images
      .load::<VmImageDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Update by name
///
/// Update a vm image in database by his name
///
/// ## Arguments
///
/// * [name](str) - Vm image name
/// * [item](VmImageUpdateDb) - Vm image to update
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [VmImageDb](VmImageDb)
///
pub(crate) async fn update_by_name(
  name: &str,
  item: &VmImageUpdateDb,
  pool: &Pool,
) -> IoResult<VmImageDb> {
  use crate::schema::vm_images;
  let item = item.clone();
  let name = name.to_owned();
  super::generic::update_by_id_with_res::<
    vm_images::table,
    VmImageUpdateDb,
    _,
    VmImageDb,
  >(name, item, pool)
  .await
}
