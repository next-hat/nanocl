use nanocl_macros_getters::repository_create;
use ntex::web;
use diesel::{
  prelude::*,
  associations::HasTable,
  query_builder::{InsertStatement, AsQuery},
  query_dsl::LoadQuery,
};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

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
/// - [item](HttpMetricDbModel) - Http metric item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](HttpMetricDbModel) - The created http metric item
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &HttpMetricDbModel,
  pool: &Pool,
) -> IoResult<HttpMetricDbModel> {
  use crate::schema::http_metrics::dsl;
  let item = item.clone();
  let item = repository_create!(dsl::http_metrics, item, pool, "HttpMetric");
  // let pool = pool.clone();
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let res = diesel::insert_into(dsl::http_metrics)
  //     .values(item)
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "HttpMetric"))?;
  //   Ok::<_, IoError>(res)
  // })
  // .await?;
  Ok(item)
}

/// ## List
///
/// List http metrics from database with given filter.
///
/// ## Arguments
///
/// - [filter](HttpMetricListQuery) - Http metric filter
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<HttpMetricDbModel>) - The list of http metrics
///   - [Err](IoError) - Error during the operation
///
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

/// ## Count
///
/// Count http metrics from database with given filter.
///
/// ## Arguments
///
/// - [filter](HttpMetricCountQuery) - Http metric filter
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericCount) - The count of http metrics
///   - [Err](IoError) - Error during the operation
///
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
