use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use crate::utils;
use crate::models::{Pool, NodeDb};

/// ## Create
///
/// Create a new node item in database
///
/// ## Arguments
///
/// * [node](NodeDb) - Node item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [NodeDb](NodeDb)
///
pub(crate) async fn create(node: &NodeDb, pool: &Pool) -> IoResult<NodeDb> {
  use crate::schema::nodes;
  let node: NodeDb = node.clone();
  super::generic::insert_with_res::<nodes::table, NodeDb, NodeDb>(node, pool)
    .await
}

/// ## Find by name
///
/// Find a node by name in database
///
/// ## Arguments
///
/// * [name](str) - Node name
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [NodeDb](NodeDb)
///
pub(crate) async fn find_by_name(name: &str, pool: &Pool) -> IoResult<NodeDb> {
  use crate::schema::nodes;
  let name = name.to_owned();
  super::generic::find_by_id::<nodes::table, _, _>(name, pool).await
}

/// ## Create if not exists
///
/// Create a node if not exists in database from a `NodeDb`.
///
/// ## Arguments
///
/// * [node](NodeDb) - Node item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [NodeDb](NodeDb)
///
pub(crate) async fn create_if_not_exists(
  node: &NodeDb,
  pool: &Pool,
) -> IoResult<NodeDb> {
  match find_by_name(&node.name, pool).await {
    Err(_) => create(node, pool).await,
    Ok(node) => Ok(node),
  }
}

/// ## List
///
/// List all nodes in database
///
/// ## Arguments
///
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [NodeDb](NodeDb)
///
pub(crate) async fn list(pool: &Pool) -> IoResult<Vec<NodeDb>> {
  use crate::schema::nodes;
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = nodes::dsl::nodes
      .load::<NodeDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## List unless
///
/// List all nodes in database unless the given name
///
/// ## Arguments
///
/// * [name](str) - Node name
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [NodeDb](NodeDb)
///
pub(crate) async fn list_unless(
  name: &str,
  pool: &Pool,
) -> IoResult<Vec<NodeDb>> {
  use crate::schema::nodes;
  let name = name.to_owned();
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = nodes::dsl::nodes
      .filter(nodes::dsl::name.ne(name))
      .load::<NodeDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}
