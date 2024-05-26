use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::system::Event;

use crate::{
  gen_sql_order_by, gen_sql_multiple, gen_sql_query,
  schema::events,
  models::{ColumnType, EventDb},
};

use super::generic::*;

impl RepositoryBase for EventDb {
  fn get_columns<'a>(
  ) -> std::collections::HashMap<&'a str, (crate::models::ColumnType, &'a str)>
  {
    std::collections::HashMap::from([
      ("key", (ColumnType::Uuid, "events.key")),
      (
        "reporting_node",
        (ColumnType::Text, "events.reporting_node"),
      ),
      ("kind", (ColumnType::Text, "events.kind")),
      ("action", (ColumnType::Text, "events.action")),
      ("reason", (ColumnType::Text, "events.reason")),
      // ("created_at", (ColumnType::Timestamp, "events.created_at")),
    ])
  }
}

impl RepositoryCreate for EventDb {}

impl RepositoryReadBy for EventDb {
  type Output = EventDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &nanocl_stubs::generic::GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  >
  where
    Self::Output: Sized,
  {
    let mut query = events::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    }
    if is_multiple {
      gen_sql_multiple!(query, events::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for EventDb {
  fn gen_count_query(
    filter: &nanocl_stubs::generic::GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = events::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for EventDb {
  type NewOutput = Event;

  fn transform(input: Self::Output) -> IoResult<Self::NewOutput> {
    Self::NewOutput::try_from(input)
  }
}
