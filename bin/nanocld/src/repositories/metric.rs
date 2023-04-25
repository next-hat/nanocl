use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{Pool, MetricDbModel, MetricInsertDbModel};

pub async fn create(
  item: &MetricInsertDbModel,
  pool: &Pool,
) -> IoResult<MetricDbModel> {
  use crate::schema::metrics::dsl;

  let item = item.clone();
  let pool = pool.clone();

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::insert_into(dsl::metrics)
      .values(item)
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Metric"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(item)
}

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
