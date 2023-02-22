use ntex::web;
use diesel::prelude::*;

use crate::utils;
use crate::models::{NodeDbModel, Pool};
use crate::error::HttpResponseError;

use super::error::{db_error, db_blocking_error};

pub async fn create(
  node: NodeDbModel,
  pool: &Pool,
) -> Result<NodeDbModel, HttpResponseError> {
  use crate::schema::nodes::dsl;

  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::insert_into(dsl::nodes)
      .values(&node)
      .get_result(&mut conn)
      .map_err(db_error("nodes"))?;

    Ok::<_, HttpResponseError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn find_by_name(
  name: &str,
  pool: &Pool,
) -> Result<NodeDbModel, HttpResponseError> {
  use crate::schema::nodes::dsl;

  let name = name.to_owned();
  let pool = pool.clone();
  let exists = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::nodes
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
      .map_err(db_error("nodes"))?;

    Ok::<_, HttpResponseError>(item)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(exists)
}

pub async fn create_if_not_exists(
  node: &NodeDbModel,
  pool: &Pool,
) -> Result<NodeDbModel, HttpResponseError> {
  match find_by_name(&node.name, pool).await {
    Err(_) => create(node.clone(), pool).await,
    Ok(node) => Ok(node),
  }
}
