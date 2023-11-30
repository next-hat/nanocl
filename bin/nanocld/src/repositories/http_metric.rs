use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericCount;
use nanocl_stubs::http_metric::HttpMetricCountQuery;

use crate::utils;
use crate::models::Pool;

/// Count http metrics from database with given filter.
pub(crate) async fn count(
  filter: &HttpMetricCountQuery,
  pool: &Pool,
) -> IoResult<GenericCount> {
  use crate::schema::http_metrics;
  let filter = filter.clone();
  let pool = Arc::clone(pool);
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
