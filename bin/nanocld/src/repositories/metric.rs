use diesel::prelude::*;
use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, MetricDb},
  schema::metrics,
};

use super::generic::*;

impl RepositoryBase for MetricDb {
  fn get_columns<'a>(
  ) -> std::collections::HashMap<&'a str, (ColumnType, &'a str)> {
    std::collections::HashMap::from([
      ("key", (ColumnType::Uuid, "metrics.key")),
      ("node_name", (ColumnType::Text, "metrics.node_name")),
      ("kind", (ColumnType::Text, "metrics.kind")),
      ("data", (ColumnType::Json, "metrics.data")),
      (
        "created_at",
        (ColumnType::Timestamptz, "metrics.created_at"),
      ),
    ])
  }
}

impl RepositoryCreate for MetricDb {}

impl RepositoryReadBy for MetricDb {
  type Output = MetricDb;

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
    let mut query = metrics::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(metrics::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for MetricDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let mut query = metrics::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}
