use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericCount;
use nanocl_stubs::http_metric::{HttpMetricListQuery, HttpMetricCountQuery};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, HttpMetricDbModel};

pub async fn create(
  item: &HttpMetricDbModel,
  pool: &Pool,
) -> IoResult<HttpMetricDbModel> {
  use crate::schema::http_metrics::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::insert_into(dsl::http_metrics)
      .values(item)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "HttpMetric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(item)
}

pub async fn list(
  filter: &HttpMetricListQuery,
  pool: &Pool,
) -> IoResult<Vec<HttpMetricDbModel>> {
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
      .map_err(|err| err.map_err_context(|| "HttpMetric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(items)
}

pub async fn count(
  filter: &HttpMetricCountQuery,
  pool: &Pool,
) -> IoResult<GenericCount> {
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
      .map_err(|err| err.map_err_context(|| "HttpMetric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(count)
}
