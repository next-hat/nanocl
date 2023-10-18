use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::FromIo;
use nanocl_utils::io_error;

use nanocl_stubs::{generic, http_metric};

use crate::{utils, models};

/// ## Create
///
/// Create a new http metric item in database
///
/// ## Arguments
///
/// - [item](models::HttpMetricDbModel) - Http metric item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::HttpMetricDbModel) - The created http metric item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &models::HttpMetricDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::HttpMetricDbModel> {
  let item = item.clone();
  utils::repository::generic_insert_with_res(pool, item).await
}

/// ## List
///
/// List http metrics from database with given filter.
///
/// ## Arguments
///
/// - [filter](http_metric::HttpMetricListQuery) - Http metric filter
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::HttpMetricDbModel>) - The list of http metrics
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list(
  filter: &http_metric::HttpMetricListQuery,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::HttpMetricDbModel>> {
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
    Ok::<_, io_error::IoError>(res)
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
/// - [filter](http_metric::HttpMetricCountQuery) - Http metric filter
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericCount) - The count of http metrics
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn count(
  filter: &http_metric::HttpMetricCountQuery,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericCount> {
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
      .map(|count: i64| generic::GenericCount { count })
      .map_err(|err| err.map_err_context(|| "HttpMetric"))?;
    Ok::<_, io_error::IoError>(res)
  })
  .await?;
  Ok(count)
}
