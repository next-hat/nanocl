use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  gen_multiple, gen_where4string, models::NamespaceDb, schema::namespaces,
};

use super::generic::*;

impl RepositoryBase for NamespaceDb {}

impl RepositoryCreate for NamespaceDb {}

impl RepositoryDelByPk for NamespaceDb {}

impl RepositoryReadBy for NamespaceDb {
  type Output = NamespaceDb;

  fn get_pk() -> &'static str {
    "name"
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
    let mut query = namespaces::table.into_boxed();
    if let Some(name) = r#where.get("name") {
      gen_where4string!(query, namespaces::name, name);
    }
    if is_multiple {
      gen_multiple!(query, namespaces::created_at, filter);
    }
    query
  }
}
