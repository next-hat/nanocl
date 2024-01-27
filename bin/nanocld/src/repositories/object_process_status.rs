use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  models::{ObjPsStatusDb, ObjPsStatusUpdate},
  schema::object_process_statuses,
  gen_where4string, gen_multiple,
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
    let r#where = filter.r#where.clone().unwrap_or_default();
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
