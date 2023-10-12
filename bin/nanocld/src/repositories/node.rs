use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use crate::{utils, schema, models};

/// ## Create
///
/// Create a new node item in database
///
/// ## Arguments
///
/// - [node](models::NodeDbModel) - Node item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::NodeDbModel) - The created node item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  node: &models::NodeDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::NodeDbModel> {
  let node: models::NodeDbModel = node.clone();
  utils::repository::generic_insert_with_res::<
    schema::nodes::table,
    models::NodeDbModel,
    models::NodeDbModel,
  >(pool, node)
  .await
}

/// ## Find by name
///
/// Find a node by name in database
///
/// ## Arguments
///
/// - [name](str) - Node name
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::NodeDbModel) - The node item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_name(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::NodeDbModel> {
  let name = name.to_owned();

  utils::repository::generic_find_by_id::<schema::nodes::table, _, _>(
    pool, name,
  )
  .await
}

/// ## Create if not exists
///
/// Create a node if not exists in database from a `models::NodeDbModel`.
///
/// ## Arguments
///
/// - [node](models::NodeDbModel) - Node item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::NodeDbModel) - The created node item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create_if_not_exists(
  node: &models::NodeDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::NodeDbModel> {
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
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::NodeDbModel>) - The list of node items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list(
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::NodeDbModel>> {
  use crate::schema::nodes::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::nodes
      .load::<models::NodeDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;

    Ok::<_, io_error::IoError>(items)
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
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::NodeDbModel>) - The list of node items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_unless(
  name: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::NodeDbModel>> {
  use crate::schema::nodes::dsl;
  let name = name.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = dsl::nodes
      .filter(dsl::name.ne(name))
      .load::<models::NodeDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;

    Ok::<_, io_error::IoError>(items)
  })
  .await?;
  Ok(items)
}
