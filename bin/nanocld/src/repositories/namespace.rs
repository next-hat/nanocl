use diesel::prelude::*;
use nanocl_stubs::generic::GenericFilter;

use crate::{models::NamespaceDb, schema::namespaces};

use super::generic::*;

impl RepositoryBase for NamespaceDb {}

impl RepositoryCreate for NamespaceDb {}

impl RepositoryDelete for NamespaceDb {
  fn gen_del_query(
    _filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable,
  {
    let query = diesel::delete(namespaces::table).into_boxed();
    query
  }
}

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
