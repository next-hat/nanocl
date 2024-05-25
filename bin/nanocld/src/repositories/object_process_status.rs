use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::{generic::GenericFilter, system::ObjPsStatusKind};

use crate::{
  gen_multiple, gen_where4string,
  models::{ObjPsStatusDb, ObjPsStatusUpdate, Pool},
  schema::object_process_statuses,
};

use super::generic::*;

impl RepositoryBase for ObjPsStatusDb {}

impl RepositoryCreate for ObjPsStatusDb {}

impl RepositoryDelByPk for ObjPsStatusDb {}

impl RepositoryUpdate for ObjPsStatusDb {
  type UpdateItem = ObjPsStatusUpdate;
}

impl RepositoryReadBy for ObjPsStatusDb {
  type Output = ObjPsStatusDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let condition = filter.r#where.clone().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = object_process_statuses::table.into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, object_process_statuses::key, value);
    }
    if let Some(value) = r#where.get("wanted") {
      gen_where4string!(query, object_process_statuses::wanted, value);
    }
    if let Some(value) = r#where.get("prev_wanted") {
      gen_where4string!(query, object_process_statuses::prev_wanted, value);
    }
    if let Some(value) = r#where.get("actual") {
      gen_where4string!(query, object_process_statuses::actual, value);
    }
    if let Some(value) = r#where.get("prev_actual") {
      gen_where4string!(query, object_process_statuses::prev_actual, value);
    }
    if is_multiple {
      gen_multiple!(query, object_process_statuses::created_at, filter);
    }
    query
  }
}

impl ObjPsStatusDb {
  pub async fn update_actual_status(
    key: &str,
    status: &ObjPsStatusKind,
    pool: &Pool,
  ) -> IoResult<()> {
    let curr_status = ObjPsStatusDb::read_by_pk(&key, pool).await?;
    let status = status.to_string();
    if curr_status.actual == status {
      return Ok(());
    }
    let new_status = ObjPsStatusUpdate {
      actual: Some(status),
      prev_actual: Some(curr_status.actual),
      ..Default::default()
    };
    ObjPsStatusDb::update_pk(key, new_status, pool).await?;
    Ok(())
  }
}
