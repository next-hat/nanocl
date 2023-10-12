use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::generic;

use crate::{utils, models, schema};

/// ## Create
///
/// Create a vm image in database for given `models::VmImageDbModel`
///
/// ## Arguments
///
/// - [item](models::VmImageDbModel) - Vm image item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::VmImageDbModel) - Vm image created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &models::VmImageDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::VmImageDbModel> {
  let item = item.clone();

  utils::repository::generic_insert_with_res(pool, item).await
}

/// ## Find by name
///
/// Find a vm image by his name in database and return a `models::VmImageDbModel`
///
/// ## Arguments
///
/// - [name](str) - Vm image name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::VmImageDbModel) - Vm image found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::VmImageDbModel> {
  let name = name.to_owned();

  utils::repository::generic_find_by_id::<schema::vm_images::table, _, _>(
    pool, name,
  )
  .await
}

/// ## Find by parent
///
/// Find all vm images in database by his parent and return a `Vec<models::VmImageDbModel>`
///
/// ## Arguments
///
/// - [parent](str) - Vm image parent
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::VmImageDbModel>) - Vm images found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_parent(
  parent: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::VmImageDbModel>> {
  use crate::schema::vm_images::dsl;
  let parent = parent.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
      .filter(dsl::parent.eq(&parent))
      .load::<models::VmImageDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, io_error::IoError>(items)
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
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Vm image deleted
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let name = name.to_owned();

  utils::repository::generic_delete_by_id::<schema::vm_images::table, _>(
    pool, name,
  )
  .await
}

/// ## List
///
/// List all vm images in database and return a `Vec<models::VmImageDbModel>`
///
/// ## Arguments
///
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::VmImageDbModel>) - Vm images found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list(
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::VmImageDbModel>> {
  use crate::schema::vm_images::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::vm_images
      .load::<models::VmImageDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmImage"))?;
    Ok::<_, io_error::IoError>(items)
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
/// - [item](models::VmImageUpdateDbModel) - Vm image to update
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::VmImageDbModel) - Vm image updated
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn update_by_name(
  name: &str,
  item: &models::VmImageUpdateDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::VmImageDbModel> {
  let item = item.clone();
  let name = name.to_owned();

  utils::repository::generic_update_by_id_with_res::<
    schema::vm_images::table,
    models::VmImageUpdateDbModel,
    _,
    models::VmImageDbModel,
  >(pool, name, item)
  .await
}
