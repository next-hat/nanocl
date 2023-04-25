//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::{NamespacePartial, NamespaceListQuery};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, NamespaceDbModel};

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
  item: &NamespacePartial,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  use crate::schema::namespaces::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = NamespaceDbModel {
      name: item.name,
      created_at: chrono::Utc::now().naive_utc(),
    };
    diesel::insert_into(dsl::namespaces)
      .values(&item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(item)
  })
  .await?;

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
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::namespaces::dsl;

  let name = name.to_owned();
  let pool = pool.clone();

  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let count = diesel::delete(dsl::namespaces.filter(dsl::name.eq(name)))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(count)
  })
  .await?;

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
  name: &str,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  use crate::schema::namespaces::dsl;

  let name = name.to_owned();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::namespaces
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(item)
  })
  .await?;

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
pub async fn exist_by_name(name: &str, pool: &Pool) -> IoResult<bool> {
  use crate::schema::namespaces::dsl;

  let name = name.to_owned();
  let pool = pool.clone();

  let exist = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let exist = Arc::new(dsl::namespaces)
      .filter(dsl::name.eq(name))
      .get_result::<NamespaceDbModel>(&mut conn)
      .optional()
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(exist)
  })
  .await?;

  Ok(exist.is_some())
}
