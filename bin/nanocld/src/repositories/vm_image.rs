use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, VmImageDbModel, VmImageUpdateDbModel};

/// ## Create
///
/// Create a vm image in database for given `VmImageDbModel`
///
/// ## Arguments
///
/// - [item](VmImageDbModel) - Vm image item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmImageDbModel) - Vm image created
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &VmImageDbModel,
  pool: &Pool,
) -> IoResult<VmImageDbModel> {
  use crate::schema::vm_images::dsl;
  let item = item.clone();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::insert_into(dsl::vm_images)
      .values(&item)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(item)
}

/// ## Find by name
///
/// Find a vm image by his name in database and return a `VmImageDbModel`
///
/// ## Arguments
///
/// - [name](str) - Vm image name
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmImageDbModel) - Vm image found
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<VmImageDbModel> {
  use crate::schema::vm_images::dsl;
  let name = name.to_owned();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::vm_images
      .filter(dsl::name.eq(&name))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(item)
}

/// ## Find by parent
///
/// Find all vm images in database by his parent and return a `Vec<VmImageDbModel>`
///
/// ## Arguments
///
/// - [parent](str) - Vm image parent
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<VmImageDbModel>) - Vm images found
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_parent(
  parent: &str,
  pool: &Pool,
) -> IoResult<Vec<VmImageDbModel>> {
  use crate::schema::vm_images::dsl;
  let parent = parent.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
      .filter(dsl::parent.eq(&parent))
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
/// - [name](str) - Vm image name
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Vm image deleted
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_name(name: &str, pool: &Pool) -> IoResult<()> {
  use crate::schema::vm_images::dsl;
  let name = name.to_owned();
  let pool = pool.clone();
  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::delete(dsl::vm_images.filter(dsl::name.eq(name)))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(())
  })
  .await?;
  Ok(())
}

/// ## List
///
/// List all vm images in database and return a `Vec<VmImageDbModel>`
///
/// ## Arguments
///
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<VmImageDbModel>) - Vm images found
///   - [Err](IoError) - Error during the operation
///
pub async fn list(pool: &Pool) -> IoResult<Vec<VmImageDbModel>> {
  use crate::schema::vm_images::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
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
/// - [name](str) - Vm image name
/// - [item](VmImageUpdateDbModel) - Vm image to update
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmImageDbModel) - Vm image updated
///   - [Err](IoError) - Error during the operation
///
pub async fn update_by_name(
  name: &str,
  item: &VmImageUpdateDbModel,
  pool: &Pool,
) -> IoResult<VmImageDbModel> {
  use crate::schema::vm_images::dsl;
  let name = name.to_owned();
  let item = item.clone();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::update(dsl::vm_images.filter(dsl::name.eq(name)))
      .set(item)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(item)
}
