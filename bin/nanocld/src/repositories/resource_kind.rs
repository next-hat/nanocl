use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;

use crate::utils;
use crate::models::{
  Pool, ResourceKindPartial, ResourceKindDbModel, ResourceKindVersionDbModel,
};

/// ## Create version
///
/// Create a resource kind with his given version in database
/// This allow custom Kind resource to be created and used in the system
///
/// ## Arguments
///
/// * [item](ResourceKindPartial) - Resource kind item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceKindDbModel) - Resource kind created
///   * [Err](IoError) - Error during the operation
///
pub async fn create_version(
  item: &ResourceKindPartial,
  pool: &Pool,
) -> IoResult<ResourceKindVersionDbModel> {
  let kind_version = ResourceKindVersionDbModel {
    resource_kind_name: item.name.clone(),
    version: item.version.clone(),
    schema: item.schema.clone(),
    url: item.url.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };
  super::generic::generic_insert_with_res(pool, kind_version).await
}

/// ## Get version
///
/// Get a resource kind for his given version in database
///
/// ## Arguments
///
/// * [name](str) - Resource kind name
/// * [version](str) - Resource kind version
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceKindVersionDbModel) - Resource kind version
///   * [Err](IoError) - Error during the operation
///
pub async fn get_version(
  name: &str,
  version: &str,
  pool: &Pool,
) -> IoResult<ResourceKindVersionDbModel> {
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
    Ok::<_, IoError>(item)
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
/// * [name](str) - Resource kind name
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceKindDbModel) - Resource kind
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<ResourceKindDbModel> {
  use crate::schema::resource_kinds;
  let name = name.to_owned();
  super::generic::generic_find_by_id::<resource_kinds::table, _, _>(pool, name)
    .await
}

/// ## Create
///
/// Create a resource kind in database from a `ResourceKindPartial`
///
/// ## Arguments
///
/// * [item](ResourceKindPartial) - Resource kind item
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ResourceKindDbModel) - Resource kind created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &ResourceKindPartial,
  pool: &Pool,
) -> IoResult<ResourceKindDbModel> {
  let kind = ResourceKindDbModel {
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };

  super::generic::generic_insert_with_res(pool, kind).await
}

/// ## Delete version
///
/// Delete a resource kind version from database
///
/// ## Arguments
///
/// * [name](str) - Resource kind name
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Resource kind version deleted
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_version(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::resource_kind_versions;
  let name = name.to_owned();
  super::generic::generic_delete::<resource_kind_versions::table, _>(
    pool,
    resource_kind_versions::resource_kind_name.eq(name),
  )
  .await
}

/// ## Delete
///
/// Delete a resource kind from database with all his versions
///
/// ## Arguments
///
/// * [name](str) - Resource kind name
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - Resource kind deleted
///   * [Err](IoError) - Error during the operation
///
pub async fn delete(name: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::resource_kinds;
  let name = name.to_owned();
  super::generic::generic_delete_by_id::<resource_kinds::table, _>(pool, name)
    .await
}
