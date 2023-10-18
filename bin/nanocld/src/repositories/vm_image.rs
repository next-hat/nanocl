use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::{Pool, VmImageDbModel, VmImageUpdateDbModel};

/// ## Create
///
/// Create a vm image in database for given `VmImageDbModel`
///
/// ## Arguments
///
/// * [item](VmImageDbModel) - Vm image item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](VmImageDbModel) - Vm image created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &VmImageDbModel,
  pool: &Pool,
) -> IoResult<VmImageDbModel> {
  let item = item.clone();
  super::generic::generic_insert_with_res(pool, item).await
}

/// ## Find by name
///
/// Find a vm image by his name in database and return a `VmImageDbModel`
///
/// ## Arguments
///
/// * [name](str) - Vm image name
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](VmImageDbModel) - Vm image found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<VmImageDbModel> {
  use crate::schema::vm_images;
  let name = name.to_owned();
  super::generic::generic_find_by_id::<vm_images::table, _, _>(pool, name).await
}

/// ## Find by parent
///
/// Find all vm images in database by his parent and return a `Vec<VmImageDbModel>`
///
/// ## Arguments
///
/// * [parent](str) - Vm image parent
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<VmImageDbModel>) - Vm images found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_parent(
  parent: &str,
  pool: &Pool,
) -> IoResult<Vec<VmImageDbModel>> {
  use crate::schema::vm_images;
  let parent = parent.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = vm_images::dsl::vm_images
      .filter(vm_images::dsl::parent.eq(&parent))
      .load::<VmImageDbModel>(&mut conn)
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Vm image deleted
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::vm_images;
  let name = name.to_owned();
  super::generic::generic_delete_by_id::<vm_images::table, _>(pool, name).await
}

/// ## List
///
/// List all vm images in database and return a `Vec<VmImageDbModel>`
///
/// ## Arguments
///
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<VmImageDbModel>) - Vm images found
///   * [Err](IoError) - Error during the operation
///
pub async fn list(pool: &Pool) -> IoResult<Vec<VmImageDbModel>> {
  use crate::schema::vm_images;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = vm_images::dsl::vm_images
      .load::<VmImageDbModel>(&mut conn)
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
/// * [item](VmImageUpdateDbModel) - Vm image to update
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](VmImageDbModel) - Vm image updated
///   * [Err](IoError) - Error during the operation
///
pub async fn update_by_name(
  name: &str,
  item: &VmImageUpdateDbModel,
  pool: &Pool,
) -> IoResult<VmImageDbModel> {
  use crate::schema::vm_images;
  let item = item.clone();
  let name = name.to_owned();
  super::generic::generic_update_by_id_with_res::<
    vm_images::table,
    VmImageUpdateDbModel,
    _,
    VmImageDbModel,
  >(pool, name, item)
  .await
}
