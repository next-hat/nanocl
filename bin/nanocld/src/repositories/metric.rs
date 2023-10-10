use nanocl_macros_getters::{repository_create, repository_create_with_res};
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, MetricDbModel, MetricInsertDbModel};

/// ## Create
///
/// Create a new metric item in database
///
/// ## Arguments
///
/// - [item](MetricInsertDbModel) - Metric item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](MetricDbModel) - The created metric item
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &MetricInsertDbModel,
  pool: &Pool,
) -> IoResult<MetricDbModel> {
  use crate::schema::metrics::dsl;
  // let item = *item;
  let item = item.clone();
  let result: MetricDbModel =
    repository_create_with_res!(dsl::metrics, item, pool, "Metric");
  // let pool = pool.clone();
  // let result = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let res = diesel::insert_into(dsl::metrics)
  //     .values(item)
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Metric"))?;
  //   Ok::<_, IoError>(res)
  // })
  // .await?;
  Ok(result)
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
/// - [kind](str) - Metric kind
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<MetricDbModel>) - The list of metrics
///   - [Err](IoError) - Error during the operation
///
pub async fn list_by_kind(
  kind: &str,
  pool: &Pool,
) -> IoResult<Vec<MetricDbModel>> {
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
      .map_err(|err| err.map_err_context(|| "Metric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;
  Ok(items)
}
