use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{models::NamespaceDb, schema::namespaces};

use super::generic::*;

impl RepositoryBase for NamespaceDb {}

impl RepositoryCreate for NamespaceDb {}

impl RepositoryDelByPk for NamespaceDb {}

impl RepositoryRead for NamespaceDb {
  type Output = NamespaceDb;
  type Query = namespaces::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let mut query = namespaces::dsl::namespaces.into_boxed();
    if is_multiple {
      query = query.order(namespaces::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}
