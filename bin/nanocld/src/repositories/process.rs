use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  process::Process,
};

use crate::{
  gen_where4string,
  schema::processes,
  models::{Pool, ProcessDb, ProcessUpdateDb},
};

use super::generic::*;

impl RepositoryBase for ProcessDb {}

impl RepositoryCreate for ProcessDb {}

/// Implement delete_by_pk and delete_by_id for ProcessDb
impl RepositoryDelete for ProcessDb {
  fn gen_del_query(
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

impl RepositoryRead for ProcessDb {
  type Output = Process;
  type Query = processes::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = processes::dsl::processes.into_boxed();
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
    if is_multiple {
      query = query.order(processes::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl RepositoryUpdate for ProcessDb {
  type UpdateItem = ProcessUpdateDb;
}

impl ProcessDb {
  pub(crate) async fn find_by_kind_key(
    kind_key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(kind_key.to_owned()));
    ProcessDb::read(&filter, pool).await?
  }
}
