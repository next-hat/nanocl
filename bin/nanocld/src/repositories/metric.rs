use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use crate::{utils, models};

/// ## Create
///
/// Create a new metric item in database
///
/// ## Arguments
///
/// - [item](models::MetricInsertDbModel) - Metric item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::MetricDbModel) - The created metric item
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &models::MetricInsertDbModel,
  pool: &models::Pool,
) -> io_error::IoResult<models::MetricDbModel> {
  let item = item.clone();

  utils::repository::generic_insert_with_res(pool, item).await
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
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::MetricDbModel>) - The list of metrics
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_by_kind(
  kind: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::MetricDbModel>> {
  use crate::schema::metrics::dsl;
  let kind = kind.to_owned();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = dsl::metrics
      .order((dsl::node_name, dsl::created_at.desc()))
      .distinct_on(dsl::node_name)
      .filter(dsl::kind.eq(kind))
      .load::<models::MetricDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "Metric"))?;
    Ok::<_, io_error::IoError>(res)
  })
  .await?;
  Ok(items)
}
