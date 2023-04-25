use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{NodeDbModel, Pool};

pub async fn create(node: &NodeDbModel, pool: &Pool) -> IoResult<NodeDbModel> {
  use crate::schema::nodes::dsl;

  let node = node.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = diesel::insert_into(dsl::nodes)
      .values(&node)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;

    Ok::<_, IoError>(item)
  })
  .await?;

  Ok(item)
}

pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<NodeDbModel> {
  use crate::schema::nodes::dsl;

  let name = name.to_owned();
  let pool = pool.clone();
  let exists = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::nodes
      .filter(dsl::name.eq(name))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "nodes"))?;

    Ok::<_, IoError>(item)
  })
  .await?;

  Ok(exists)
}

pub async fn create_if_not_exists(
  node: &NodeDbModel,
  pool: &Pool,
) -> IoResult<NodeDbModel> {
  match find_by_name(&node.name, pool).await {
    Err(_) => create(node, pool).await,
    Ok(node) => Ok(node),
  }
}

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
