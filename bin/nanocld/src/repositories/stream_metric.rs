use nanocl_error::io::IoResult;

use crate::models::{Pool, StreamMetricDb};

/// ## Create
///
/// Create a new `StreamMetricDbModel`` item in database
///
/// ## Arguments
///
/// * [item](StreamMetricDbModel) - Http metric item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [StreamMetricDbModel](StreamMetricDbModel)
///
pub(crate) async fn create(
  item: &StreamMetricDb,
  pool: &Pool,
) -> IoResult<StreamMetricDb> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}
