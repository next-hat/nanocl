use ntex::web;
use diesel::prelude::*;

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, MetricDbModel, MetricInsertDbModel};

use super::error::{db_error, db_blocking_error};

pub async fn create(
  item: &MetricInsertDbModel,
  pool: &Pool,
) -> Result<MetricDbModel, HttpError> {
  use crate::schema::metrics::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::insert_into(dsl::metrics)
      .values(item)
      .get_result(&mut conn)
      .map_err(db_error("metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn list_by_kind(
  kind: &str,
  pool: &Pool,
) -> Result<Vec<MetricDbModel>, HttpError> {
  use crate::schema::metrics::dsl;

  let kind = kind.to_owned();
  let pool = pool.clone();

  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = dsl::metrics
      .order((dsl::node_name, dsl::created_at.desc()))
      .distinct_on(dsl::node_name)
      .filter(dsl::kind.eq(kind))
      .load::<MetricDbModel>(&mut conn)
      .map_err(db_error("metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(items)
}
