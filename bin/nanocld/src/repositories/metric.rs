use diesel::prelude::*;
use nanocl_stubs::generic::GenericFilter;

use crate::{gen_where4json, gen_where4string, models::MetricDb, schema::metrics};

use super::generic::*;

impl RepositoryBase for MetricDb {}

impl RepositoryCreate for MetricDb {}

// impl RepositoryDelByPk for MetricDb {}

impl RepositoryRead for MetricDb {
  type Output = MetricDb;
  type Query = metrics::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = metrics::dsl::metrics.into_boxed();
    if let Some(node_name) = r#where.get("node_name") {
      gen_where4string!(query, metrics::dsl::node_name, node_name);
    }
    if let Some(kind) = r#where.get("kind") {
      gen_where4string!(query, metrics::dsl::kind, kind);
    }
    if let Some(data) = r#where.get("data") {
      gen_where4json!(query, metrics::dsl::data, data);
    }
    if is_multiple {
      query = query.order(metrics::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}
