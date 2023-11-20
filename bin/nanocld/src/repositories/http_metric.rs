use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericCount;
use nanocl_stubs::http_metric::{HttpMetricListQuery, HttpMetricCountQuery};

use crate::utils;
use crate::models::{Pool, HttpMetricDbModel};

/// ## Create
///
/// Create a new http metric item in database
///
/// ## Arguments
///
/// * [item](HttpMetricDbModel) - Http metric item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [HttpMetricDbModel](HttpMetricDbModel)
///
pub(crate) async fn create(
  item: &HttpMetricDbModel,
  pool: &Pool,
) -> IoResult<HttpMetricDbModel> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}

/// ## List
///
/// List http metrics from database with given filter.
///
/// ## Arguments
///
/// * [filter](HttpMetricListQuery) - Http metric filter
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [HttpMetricDbModel](HttpMetricDbModel)
///
pub(crate) async fn list(
  filter: &HttpMetricListQuery,
  pool: &Pool,
) -> IoResult<Vec<HttpMetricDbModel>> {
  use crate::schema::http_metrics;
  let filter = filter.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut query = http_metrics::dsl::http_metrics.into_boxed().order((
      http_metrics::dsl::date_gmt,
      http_metrics::dsl::created_at.desc(),
    ));
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

/// ## Count
///
/// Count http metrics from database with given filter.
///
/// ## Arguments
///
/// * [filter](HttpMetricCountQuery) - Http metric filter
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericCount](GenericCount)
///
pub(crate) async fn count(
  filter: &HttpMetricCountQuery,
  pool: &Pool,
) -> IoResult<GenericCount> {
  use crate::schema::http_metrics;
  let filter = filter.clone();
  let pool = pool.clone();
  let count = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let mut query = http_metrics::dsl::http_metrics.into_boxed();
    if let Some(status) = filter.status {
      if let Some(status_max) = status.1 {
        query =
          query.filter(http_metrics::dsl::status.between(status.0, status_max));
      } else {
        query = query.filter(http_metrics::dsl::status.eq(status.0));
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
