use diesel::prelude::*;
use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_sql_multiple, gen_sql_where4json, gen_sql_where4string,
  gen_sql_where4uuid,
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
      // ("created_at", (ColumnType::Timestamp, "metrics.created_at")),
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
    let condition = filter.r#where.clone().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = metrics::table.into_boxed();
    if let Some(key) = r#where.get("key") {
      gen_sql_where4uuid!(query, metrics::key, key);
    }
    if let Some(node_name) = r#where.get("node_name") {
      gen_sql_where4string!(query, metrics::node_name, node_name);
    }
    if let Some(kind) = r#where.get("kind") {
      gen_sql_where4string!(query, metrics::kind, kind);
    }
    if let Some(data) = r#where.get("data") {
      gen_sql_where4json!(query, metrics::data, data);
    }
    if is_multiple {
      gen_sql_multiple!(query, metrics::dsl::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for MetricDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let condition = filter.r#where.clone().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = metrics::table.into_boxed();
    if let Some(key) = r#where.get("key") {
      gen_sql_where4uuid!(query, metrics::key, key);
    }
    if let Some(node_name) = r#where.get("node_name") {
      gen_sql_where4string!(query, metrics::node_name, node_name);
    }
    if let Some(kind) = r#where.get("kind") {
      gen_sql_where4string!(query, metrics::kind, kind);
    }
    if let Some(data) = r#where.get("data") {
      gen_sql_where4json!(query, metrics::data, data);
    }
    query.count()
  }
}
