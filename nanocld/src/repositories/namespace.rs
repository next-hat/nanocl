//! Repository to manage namespaces in database
//! We can create delete list or inspect a namespace
use ntex::web;
use diesel::prelude::*;

use nanocl_models::generic::GenericDelete;
use nanocl_models::namespace::NamespacePartial;

use crate::utils;
use crate::error::HttpResponseError;
use crate::models::{Pool, NamespaceDbModel};

use super::error::db_blocking_error;

/// Create a new namespace
///
/// ## Arguments
///
/// * [name](String) - Partial namespace
/// * [pool](Pool) - Posgresql database pool
///
/// ## Return
/// * [Result](Result)
///   * [Ok](NamespaceDbModel) - a [NamespaceDbModel](NamespaceDbModel)
///   * [Err](HttpResponseError)
///
pub async fn create(
  item: NamespacePartial,
  pool: &Pool,
) -> Result<NamespaceDbModel, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || {
    let item = NamespaceDbModel { name: item.name };
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
/// ## Arguments
///
/// * [pool](Pool) - Posgresql database pool
///
/// ## Return
/// * [Result](Result)
///   * [Ok](Vec<NamespaceDbModel>) Vector of [NamespaceDbModel](NamespaceDbModel)
///   * [Err](HttpResponseError)
///
pub async fn list(
  pool: &Pool,
) -> Result<Vec<NamespaceDbModel>, HttpResponseError> {
  use crate::schema::namespaces::dsl;

  let mut conn = utils::store::get_pool_conn(pool)?;
  let res = web::block(move || dsl::namespaces.load(&mut conn)).await;

  match res {
    Err(err) => Err(db_blocking_error(err)),
    Ok(items) => Ok(items),
  }
}

/// Delete namespace by name
///
/// ## Arguments
/// * [name](String) Name of the namespace
/// * [pool](Pool) - Posgresql database pool
///
/// ## Return
/// * [Result](Result)
///   * [Ok](GenericDelete) - a [GenericDelete](GenericDelete)
///   * [Err](HttpResponseError)
///
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

/// Find namespace by name
///
/// ## Arguments
/// * [name](String) Name of the namespace
/// * [pool](Pool) - Posgresql database pool
///
/// ## Return
/// * [Result](Result)
///   * [Ok](NamespaceDbModel) - a [NamespaceDbModel](NamespaceDbModel)
///   * [Err](HttpResponseError)
///
pub async fn find_by_name(
  name: String,
  pool: &Pool,
) -> Result<NamespaceDbModel, HttpResponseError> {
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
