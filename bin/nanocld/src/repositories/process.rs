use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  process::Process,
  generic::{GenericFilter, GenericClause},
};

use crate::{
  gen_multiple, gen_where4json, gen_where4string,
  schema::processes,
  models::{Pool, ProcessDb, ProcessUpdateDb},
};

use super::generic::*;

impl RepositoryBase for ProcessDb {}

impl RepositoryCreate for ProcessDb {}

impl RepositoryUpdate for ProcessDb {
  type UpdateItem = ProcessUpdateDb;
}

impl RepositoryDelByPk for ProcessDb {}

/// Implement delete_by_pk and delete_by_id for ProcessDb
impl RepositoryDelBy for ProcessDb {
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
    let condition = filter.r#where.to_owned().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = diesel::delete(processes::table).into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::kind_key, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, processes::data, value);
    }
    query
  }
}

impl RepositoryReadBy for ProcessDb {
  type Output = ProcessDb;

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
    let condition = filter.r#where.to_owned().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = processes::table.into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::kind_key, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, processes::data, value);
    }
    if is_multiple {
      gen_multiple!(query, processes::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for ProcessDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let condition = filter.r#where.to_owned().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = processes::table.into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::kind_key, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, processes::data, value);
    }
    query.count()
  }
}

impl RepositoryReadByTransform for ProcessDb {
  type NewOutput = Process;

  fn transform(input: ProcessDb) -> IoResult<Self::NewOutput> {
    input.try_into()
  }
}

impl ProcessDb {
  pub async fn read_by_kind_key(
    kind_key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(kind_key.to_owned()));
    ProcessDb::transform_read_by(&filter, pool).await
  }
}

impl ProcessDb {
  pub async fn list_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = GenericFilter::new().r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Config": {
          "Labels": {
            "io.nanocl.n": name
          }
        }
      })),
    );
    ProcessDb::transform_read_by(&filter, pool).await
  }
}
