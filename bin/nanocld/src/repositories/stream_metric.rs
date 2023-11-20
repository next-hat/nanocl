use nanocl_error::io::IoResult;

use crate::models::{Pool, StreamMetricDbModel};

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
  item: &StreamMetricDbModel,
  pool: &Pool,
) -> IoResult<StreamMetricDbModel> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}
