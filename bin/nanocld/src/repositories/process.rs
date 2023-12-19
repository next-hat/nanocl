use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{schema::processes, models::ProcessDb, gen_where4string};

use super::generic::*;

impl RepositoryBase for ProcessDb {}

impl RepositoryCreate for ProcessDb {}

/// Implement delete_by_pk and delete_by_id for ProcessDb
impl RepositoryDelete for ProcessDb {
  fn get_delete_query(
    filter: &GenericFilter,
  ) -> diesel::query_builder::BoxedDeleteStatement<
    'static,
    diesel::pg::Pg,
    <Self as diesel::associations::HasTable>::Table,
  >
  where
    Self: diesel::associations::HasTable,
  {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = diesel::delete(processes::dsl::processes).into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::dsl::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::dsl::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::dsl::kind_key, value);
    }
    query
  }
}
