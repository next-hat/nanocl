use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::{NamespacePartial, NamespaceListQuery};

use crate::utils;
use crate::models::{Pool, NamespaceDbModel};

/// ## Create
///
/// Create a namespace in database
///
/// ## Arguments
///
/// * [item](NamespacePartial) - Namespace to create
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](NamespaceDbModel) - Namespace created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &NamespacePartial,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  let item = NamespaceDbModel {
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };
  super::generic::insert_with_res(item, pool).await
}

/// ## List
///
/// List all namespaces in database
///
/// ## Arguments
///
/// * [query](NamespaceListQuery) - Namespace list query
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<NamespaceDbModel>) - List of namespaces
///   * [Err](IoError) - Error during the operation
///
pub async fn list(
  query: &NamespaceListQuery,
  pool: &Pool,
) -> IoResult<Vec<NamespaceDbModel>> {
  use crate::schema::namespaces::dsl;
  let query = query.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut sql = dsl::namespaces.into_boxed();
    if let Some(name) = &query.name {
      sql = sql.filter(dsl::name.ilike(format!("%{name}%")));
    }
    if let Some(limit) = query.limit {
      sql = sql.limit(limit);
    }
    if let Some(offset) = query.offset {
      sql = sql.offset(offset);
    }
    let items = sql
      .load(&mut conn)
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Delete by name
///
/// Delete a namespace by name in database
///
/// ## Arguments
///
/// * [name](str) - Name of the namespace to delete
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Number of deleted namespaces
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::namespaces;
  let name = name.to_owned();
  super::generic::delete_by_id::<namespaces::table, _>(name, pool).await
}

/// ## Find by name
///
/// Find a namespace by name in database
///
/// ## Arguments
///
/// * [name](str) - Name of the namespace to find
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](NamespaceDbModel) - Namespace found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  use crate::schema::namespaces;
  let name = name.to_owned();
  super::generic::find_by_id::<namespaces::table, _, _>(name, pool).await
}

/// ## Exist by name
///
/// Check if a namespace exist by name in database
///
/// ## Arguments
///
/// * [name](str) - Name of the namespace to check
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](bool) - Existence of the namespace
///   * [Err](IoError) - Error during the operation
///
pub async fn exist_by_name(name: &str, pool: &Pool) -> IoResult<bool> {
  use crate::schema::namespaces;
  let name = name.to_owned();
  let pool = pool.clone();
  let exist = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let exist = namespaces::dsl::namespaces
      .filter(namespaces::dsl::name.eq(name))
      .get_result::<NamespaceDbModel>(&mut conn)
      .optional()
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(exist)
  })
  .await?;
  Ok(exist.is_some())
}
