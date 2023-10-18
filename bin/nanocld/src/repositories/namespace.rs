//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::{generic, namespace};

use crate::{utils, schema, models};

/// ## Create
///
/// Create a namespace in database
///
/// ## Arguments
///
/// - [item](namespace::NamespacePartial) - Namespace to create
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::NamespaceDbModel) - Namespace created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &namespace::NamespacePartial,
  pool: &models::Pool,
) -> io_error::IoResult<models::NamespaceDbModel> {
  let item = models::NamespaceDbModel {
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
  };

  utils::repository::generic_insert_with_res(pool, item).await
}

/// ## List
///
/// List all namespaces in database
///
/// ## Arguments
///
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::NamespaceDbModel>) - List of namespaces
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list(
  query: &namespace::NamespaceListQuery,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::NamespaceDbModel>> {
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
    Ok::<_, io_error::IoError>(items)
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
/// - [name](str) - Name of the namespace to delete
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericDelete) - Number of deleted namespaces
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let name = name.to_owned();

  utils::repository::generic_delete_by_id::<schema::namespaces::table, _>(
    pool, name,
  )
  .await
}

/// ## Find by name
///
/// Find a namespace by name in database
///
/// ## Arguments
///
/// - [name](str) - Name of the namespace to find
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::NamespaceDbModel) - Namespace found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::NamespaceDbModel> {
  let name = name.to_owned();

  utils::repository::generic_find_by_id::<schema::namespaces::table, _, _>(
    pool, name,
  )
  .await
}

/// ## Exist by name
///
/// Check if a namespace exist by name in database
///
/// ## Arguments
///
/// - [name](str) - Name of the namespace to check
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](bool) - Existence of the namespace
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn exist_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<bool> {
  use crate::schema::namespaces::dsl;
  let name = name.to_owned();
  let pool = pool.clone();
  let exist = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    //TODO: remove Arc ?
    let exist = Arc::new(dsl::namespaces)
      .filter(dsl::name.eq(name))
      .get_result::<models::NamespaceDbModel>(&mut conn)
      .optional()
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, io_error::IoError>(exist)
  })
  .await?;
  Ok(exist.is_some())
}
