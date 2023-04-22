use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericCount;
use nanocl_stubs::http_metric::{HttpMetricListQuery, HttpMetricCountQuery};

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, HttpMetricDbModel};

use super::error::{db_error, db_blocking_error};

pub async fn create(
  item: &HttpMetricDbModel,
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

pub async fn list(
  filter: &HttpMetricListQuery,
  pool: &Pool,
) -> Result<Vec<HttpMetricDbModel>, HttpError> {
  use crate::schema::http_metrics::dsl;

  let filter = filter.clone();
  let pool = pool.clone();

  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut query = dsl::http_metrics
      .into_boxed()
      .order((dsl::date_gmt, dsl::created_at.desc()));

    if let Some(limit) = filter.limit {
      query = query.limit(limit);
    }
    if let Some(offset) = filter.offset {
      query = query.offset(offset);
    }

    let res = query
      .get_results(&mut conn)
      .map_err(db_error("http_metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(items)
}

pub async fn count(
  filter: &HttpMetricCountQuery,
  pool: &Pool,
) -> Result<GenericCount, HttpError> {
  use crate::schema::http_metrics::dsl;

  let filter = filter.clone();
  let pool = pool.clone();

  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut query = dsl::http_metrics.into_boxed();

    if let Some(status) = filter.status {
      if let Some(status_max) = status.1 {
        query = query.filter(dsl::status.between(status.0, status_max));
      } else {
        query = query.filter(dsl::status.eq(status.0));
      }
    }

    let res = query
      .count()
      .get_result(&mut conn)
      .map(|count: i64| GenericCount { count })
      .map_err(db_error("http_metrics"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(count)
}
