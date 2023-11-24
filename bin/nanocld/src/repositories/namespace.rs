use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::{NamespacePartial, NamespaceListQuery};

use crate::utils;
use crate::models::{Pool, NamespaceDb};

/// ## Create
///
/// Create a namespace in database
///
/// ## Arguments
///
/// * [item](NamespacePartial) - Namespace to create
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [NamespaceDb](NamespaceDb)
///
pub(crate) async fn create(
  item: &NamespacePartial,
  pool: &Pool,
) -> IoResult<NamespaceDb> {
  let item = NamespaceDb {
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
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [NamespaceDb](NamespaceDb)
///
pub(crate) async fn list(
  query: &NamespaceListQuery,
  pool: &Pool,
) -> IoResult<Vec<NamespaceDb>> {
  use crate::schema::namespaces::dsl;
  let query = query.clone();
  let pool = Arc::clone(pool);
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
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_name(
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
/// ## Return
///
/// [IoResult](IoResult) containing a [NamespaceDb](NamespaceDb)
///
pub(crate) async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<NamespaceDb> {
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
/// ## Return
///
/// [IoResult](IoResult) containing a [bool](bool)
///
pub(crate) async fn exist_by_name(name: &str, pool: &Pool) -> IoResult<bool> {
  use crate::schema::namespaces;
  let name = name.to_owned();
  let pool = Arc::clone(pool);
  let exist = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let exist = namespaces::dsl::namespaces
      .filter(namespaces::dsl::name.eq(name))
      .get_result::<NamespaceDb>(&mut conn)
      .optional()
      .map_err(|err| err.map_err_context(|| "Namespace"))?;
    Ok::<_, IoError>(exist)
  })
  .await?;
  Ok(exist.is_some())
}
