//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use std::sync::Arc;

use nanocl_macros_getters::{
  repository_find_by_id, repository_delete_by_id, repository_create,
};
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

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
/// - [item](NamespacePartial) - Namespace to create
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceDbModel) - Namespace created
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &NamespacePartial,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  use crate::schema::namespaces::dsl;
  let item = item.clone();
  let item = NamespaceDbModel {
    name: item.name,
    created_at: chrono::Utc::now().naive_utc(),
  };

  let item = repository_create!(dsl::namespaces, item, pool, "Namespace");
  // let pool = pool.clone();
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   diesel::insert_into(dsl::namespaces)
  //     .values(&item)
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Namespace"))?;
  //   Ok::<_, IoError>(item)
  // })
  // .await?;
  Ok(item)
}

/// ## List
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
///   - [Err](IoError) - Error during the operation
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
/// - [name](str) - Name of the namespace to delete
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - Number of deleted namespaces
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::namespaces::dsl;
  // let name = name.to_owned();
  // let pool = pool.clone();
  // let count = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let count = diesel::delete(dsl::namespaces.filter(dsl::name.eq(name)))
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Namespace"))?;
  //   Ok::<_, IoError>(count)
  // })
  // .await?;
  let count =
    repository_delete_by_id!(dsl::namespaces, name, pool, "Namespace");
  Ok(GenericDelete { count })
}

/// ## Find by name
///
/// Find a namespace by name in database
///
/// ## Arguments
///
/// - [name](str) - Name of the namespace to find
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceDbModel) - Namespace found
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<NamespaceDbModel> {
  use crate::schema::namespaces::dsl;
  // let name = name.to_owned();
  // let pool = pool.clone();
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let item = dsl::namespaces
  //     .filter(dsl::name.eq(name))
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Namespace"))?;
  //   Ok::<_, IoError>(item)
  // })
  // .await?;
  let item = repository_find_by_id!(dsl::namespaces, name, pool, "Namespace");
  Ok(item)
}

/// ## Exist by name
///
/// Check if a namespace exist by name in database
///
/// ## Arguments
///
/// - [name](str) - Name of the namespace to check
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](bool) - Existence of the namespace
///   - [Err](IoError) - Error during the operation
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
