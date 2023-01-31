//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::NamespacePartial;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{Pool, NamespaceDbModel};

use super::error::{db_error, db_blocking_error};

/// ## Create namespace
///
/// Create a namespace in database
///
/// ## Arguments
///
/// - [item](NamespacePartial) - Namespace to create
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceDbModel) - Namespace created
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
/// use nanocl_stubs::namespace::NamespacePartial;
///
/// let item = NamespacePartial {
///   name: "my-namespace".into(),
/// };
/// let namespace = repositories::namespace::create(item, &pool).await;
/// ```
///
pub async fn create(
  item: NamespacePartial,
  pool: &Pool,
) -> Result<NamespaceDbModel, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let pool = pool.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = NamespaceDbModel { name: item.name };
    diesel::insert_into(dsl::namespaces)
      .values(&item)
      .execute(&mut conn)
      .map_err(db_error("namespace"))?;
    Ok::<_, HttpResponseError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

/// ## List namespaces
///
/// List all namespaces in database
///
/// ## Arguments
///
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<NamespaceDbModel>) - List of namespaces
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let namespaces = repositories::namespace::list(&pool).await;
/// ```
///
pub async fn list(
  pool: &Pool,
) -> Result<Vec<NamespaceDbModel>, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let pool = pool.to_owned();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::namespaces
      .load(&mut conn)
      .map_err(db_error("namespace"))?;
    Ok::<_, HttpResponseError>(items)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(items)
}

/// ## Delete namespace by name
///
/// Delete a namespace by name in database
///
/// ## Arguments
///
/// - [name](String) - Name of the namespace to delete
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - Number of deleted namespaces
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let count = repositories::namespace::delete_by_name("my-namespace".into(), &pool).await;
/// ```
///
pub async fn delete_by_name(
  name: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let pool = pool.to_owned();
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = diesel::delete(dsl::namespaces.filter(dsl::name.eq(name)))
      .execute(&mut conn)
      .map_err(db_error("namespace"))?;
    Ok::<_, HttpResponseError>(count)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(GenericDelete { count })
}

/// ## Find namespace by name
///
/// Find a namespace by name in database
///
/// ## Arguments
///
/// - [name](String) - Name of the namespace to find
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceDbModel) - Namespace found
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let namespace = repositories::namespace::find_by_name("my-namespace".into(), &pool).await;
/// ```
///
pub async fn find_by_name(
  name: String,
  pool: &Pool,
) -> Result<NamespaceDbModel, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let pool = pool.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::namespaces
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
      .map_err(db_error("namespace"))?;
    Ok::<_, HttpResponseError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

/// ## Exist namespace by name
///
/// Check if a namespace exist by name in database
///
/// ## Arguments
///
/// - [name](String) - Name of the namespace to check
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](bool) - Existence of the namespace
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// let exist = repositories::namespace::exist_by_name("my-namespace".into(), &pool).await.unwrap();
/// ```
///
pub async fn exist_by_name(
  name: String,
  pool: &Pool,
) -> Result<bool, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let pool = pool.to_owned();
  let exist = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let exist = Arc::new(dsl::namespaces)
      .filter(dsl::name.eq(name))
      .get_result::<NamespaceDbModel>(&mut conn)
      .optional()
      .map_err(db_error("namespace"))?;
    Ok::<_, HttpResponseError>(exist)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(exist.is_some())
}
