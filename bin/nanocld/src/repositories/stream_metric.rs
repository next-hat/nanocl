use nanocl_utils::io_error::IoResult;

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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](StreamMetricDbModel) - The created http metric item
///   * [Err](nanocl_utils::io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &StreamMetricDbModel,
  pool: &Pool,
) -> IoResult<StreamMetricDbModel> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}
