//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use ntex::web;
use diesel::prelude::*;

use crate::models::{Pool, NamespacePartial, NamespaceItem, GenericDelete};

use crate::utils;
use crate::errors::HttpResponseError;
use super::errors::db_blocking_error;

/// Create new namespace
///
/// # Arguments
///
/// * [name](String) - Partial namespace
/// * `pool` - Posgresql database pool
///
/// # Examples
///
/// ```rust,norun
///
/// use crate::repositories;
///
/// let new_namespace = NamespaceCreate {
///   name: String::from("new-nsp"),
/// };
/// repositories::namespace::create(new_namespace, &pool).await;
/// ```
pub async fn create(
  item: NamespacePartial,
  pool: &Pool,
) -> Result<NamespaceItem, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    let item = NamespaceItem { name: item.name };
    diesel::insert_into(dsl::namespaces)
      .values(&item)
      .execute(&mut conn)?;
    Ok(item)
  })
  .await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(item) => Ok(item),
  }
}

/// List all namespace
///
/// # Arguments
///
/// * `pool` - Posgresql database pool
///
/// # Examples
///
/// ```rust,norun
///
/// use crate::repositories;
/// repositories::namespace::list(&pool).await;
/// ```
pub async fn list(
  pool: &Pool,
) -> Result<Vec<NamespaceItem>, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || dsl::namespaces.load(&mut conn)).await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(items) => Ok(items),
  }
}

/// Inspect namespace by id or name
///
/// # Arguments
///
/// * `id_or_name` Id or name of the namespace
/// * `pool` - Posgresql database pool
///
/// # Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// repositories::namespace::inspect_by_name(String::from("default"), &pool).await;
/// ```
pub async fn inspect_by_name(
  name: String,
  pool: &Pool,
) -> Result<NamespaceItem, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    dsl::namespaces
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
  })
  .await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(item) => Ok(item),
  }
}

/// Delete namespace by id or name
///
/// # Arguments
///
/// * `id_or_name` Id or name of the namespace
/// * `pool` - Posgresql database pool
///
/// # Examples
///
/// ```rust,norun
/// use crate::repositories;
///
/// repositories::namespace::delete_by_name(String::from("default"), &pool).await;
/// ```
pub async fn delete_by_name(
  name: String,
  pool: &Pool,
) -> Result<GenericDelete, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    diesel::delete(dsl::namespaces.filter(dsl::name.eq(name)))
      .execute(&mut conn)
  })
  .await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(result) => Ok(GenericDelete { count: result }),
  }
}

pub async fn find_by_name(
  name: String,
  pool: &Pool,
) -> Result<NamespaceItem, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    dsl::namespaces
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
  })
  .await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(item) => Ok(item),
  }
}
