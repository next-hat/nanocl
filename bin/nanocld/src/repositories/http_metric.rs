use ntex::web;
use diesel::prelude::*;

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, HttpMetricPartial, HttpMetricDbModel};

use super::error::{db_error, db_blocking_error};

pub async fn create(
  item: &HttpMetricPartial,
  pool: &Pool,
) -> Result<HttpMetricDbModel, HttpError> {
  use crate::schema::http_metrics::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::insert_into(dsl::http_metrics)
      .values(item)
      .get_result(&mut conn)
      .map_err(db_error("http_metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(item)
}

pub async fn list(pool: &Pool) -> Result<Vec<HttpMetricDbModel>, HttpError> {
  use crate::schema::http_metrics::dsl;

  let pool = pool.clone();

  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = dsl::http_metrics
      .order((dsl::date_gmt, dsl::created_at.desc()))
      .get_results(&mut conn)
      .map_err(db_error("http_metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(items)
}
