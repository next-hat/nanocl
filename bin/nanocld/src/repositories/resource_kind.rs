use nanocl_stubs::generic;
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use crate::{utils, schema, models};

/// ## Create version
///
/// Create a resource kind with his given version in database
/// This allow custom Kind resource to be created and used in the system
///
/// ## Arguments
///
/// - [item](models::ResourceKindPartial) - Resource kind item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::ResourceKindDbModel) - Resource kind created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create_version(
  item: &models::ResourceKindPartial,
  pool: &models::Pool,
) -> io_error::IoResult<models::ResourceKindVersionDbModel> {
  let kind_version = models::ResourceKindVersionDbModel {
    resource_kind_name: item.name.clone(),
    version: item.version.clone(),
    schema: item.schema.clone(),
    url: item.url.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };

  utils::repository::generic_insert_with_res(pool, kind_version).await
}

/// ## Get version
///
/// Get a resource kind for his given version in database
///
/// ## Arguments
///
/// - [name](str) - Resource kind name
/// - [version](str) - Resource kind version
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::ResourceKindVersionDbModel) - Resource kind version
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn get_version(
  name: &str,
  version: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::ResourceKindVersionDbModel> {
  use crate::schema::resource_kind_versions::dsl;
  let pool = pool.clone();
  let name = name.to_owned();
  let version = version.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::resource_kind_versions
      .filter(dsl::resource_kind_name.eq(&name))
      .filter(dsl::version.eq(&version))
      .get_result(&mut conn)
      .map_err(|err| {
        err.map_err_context(|| format!("Resource {name} {version}"))
      })?;
    Ok::<_, io_error::IoError>(item)
  })
  .await?;
  Ok(item)
}

/// ## Find by name
///
/// Find a resource kind by his name
///
/// ## Arguments
///
/// - [name](str) - Resource kind name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::ResourceKindDbModel) - Resource kind
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::ResourceKindDbModel> {
  let name = name.to_owned();

  utils::repository::generic_find_by_id::<schema::resource_kinds::table, _, _>(
    pool, name,
  )
  .await
}

/// ## Create
///
/// Create a resource kind in database from a `models::ResourceKindPartial`
///
/// ## Arguments
///
/// - [item](models::ResourceKindPartial) - Resource kind item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::ResourceKindDbModel) - Resource kind created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &models::ResourceKindPartial,
  pool: &models::Pool,
) -> io_error::IoResult<models::ResourceKindDbModel> {
  let kind = models::ResourceKindDbModel {
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };

  utils::repository::generic_insert_with_res(pool, kind).await
}

/// ## Delete version
///
/// Delete a resource kind version from database
///
/// ## Arguments
///
/// - [name](str) - Resource kind name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Resource kind version deleted
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_version(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let name = name.to_owned();

  utils::repository::generic_delete::<schema::resource_kind_versions::table, _>(
    pool,
    schema::resource_kind_versions::resource_kind_name.eq(name),
  )
  .await
}

/// ## Delete
///
/// Delete a resource kind from database with all his versions
///
/// ## Arguments
///
/// - [name](str) - Resource kind name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - Resource kind deleted
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let name = name.to_owned();

  utils::repository::generic_delete_by_id::<schema::resource_kinds::table, _>(
    pool, name,
  )
  .await
}
