use nanocl_macros_getters::{repository_create_with_res, repository_find_by_id};
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, NodeDbModel};

/// ## Create
///
/// Create a new node item in database
///
/// ## Arguments
///
/// - [node](NodeDbModel) - Node item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NodeDbModel) - The created node item
///   - [Err](IoError) - Error during the operation
///
pub async fn create(node: &NodeDbModel, pool: &Pool) -> IoResult<NodeDbModel> {
  use crate::schema::nodes::dsl;
  let node = node.clone();
  let item = repository_create_with_res!(dsl::nodes, node, pool, "Nodes");
  // let pool = pool.clone();
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let item = diesel::insert_into(dsl::nodes)
  //     .values(&node)
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "nodes"))?;
  //   Ok::<_, IoError>(item)
  // })
  // .await?;
  Ok(item)
}

/// ## Find by name
///
/// Find a node by name in database
///
/// ## Arguments
///
/// - [name](str) - Node name
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NodeDbModel) - The node item
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<NodeDbModel> {
  use crate::schema::nodes::dsl;
  let item = repository_find_by_id!(dsl::nodes, name, pool, "Nodes");
  // let name = name.to_owned();
  // let pool = pool.clone();
  // let exists = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let item = dsl::nodes
  //     .filter(dsl::name.eq(name))
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "nodes"))?;

  //   Ok::<_, IoError>(item)
  // })
  // .await?;
  Ok(item)
}

/// ## Create if not exists
///
/// Create a node if not exists in database from a `NodeDbModel`.
///
/// ## Arguments
///
/// - [node](NodeDbModel) - Node item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NodeDbModel) - The created node item
///   - [Err](IoError) - Error during the operation
///
pub async fn create_if_not_exists(
  node: &NodeDbModel,
  pool: &Pool,
) -> IoResult<NodeDbModel> {
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
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<NodeDbModel>) - The list of node items
///   - [Err](IoError) - Error during the operation
///
pub async fn list(pool: &Pool) -> IoResult<Vec<NodeDbModel>> {
  use crate::schema::nodes::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::nodes
      .load::<NodeDbModel>(&mut conn)
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
/// - [name](str) - Node name
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<NodeDbModel>) - The list of node items
///   - [Err](IoError) - Error during the operation
///
pub async fn list_unless(
  name: &str,
  pool: &Pool,
) -> IoResult<Vec<NodeDbModel>> {
  use crate::schema::nodes::dsl;
  let name = name.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::nodes
      .filter(dsl::name.ne(name))
      .load::<NodeDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;

    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}
