use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use crate::utils;
use crate::models::{Pool, MetricDb, MetricInsertDb};

/// ## Create
///
/// Create a new metric item in database
///
/// ## Arguments
///
/// * [item](MetricInsertDbModel) - Metric item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [MetricDbModel](MetricDbModel)
///
pub(crate) async fn create(
  item: &MetricInsertDb,
  pool: &Pool,
) -> IoResult<MetricDb> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}

/// ## List by kind
///
/// List metrics from database with given kind.
/// It can be:
/// - `CPU`
/// - `MEMORY`
/// - `DISK`
/// - `NETWORK`
///
/// ## Arguments
///
/// * [kind](str) - Metric kind
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [MetricDbModel](MetricDbModel)
///
pub(crate) async fn list_by_kind(
  kind: &str,
  pool: &Pool,
) -> IoResult<Vec<MetricDb>> {
  use crate::schema::metrics;
  let kind = kind.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = metrics::dsl::metrics
      .order((metrics::dsl::node_name, metrics::dsl::created_at.desc()))
      .distinct_on(metrics::dsl::node_name)
      .filter(metrics::dsl::kind.eq(kind))
      .load::<MetricDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "Metric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;
  Ok(items)
}
