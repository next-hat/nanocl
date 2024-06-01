use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::IoResult;

use nanocl_stubs::{
  process::Process,
  generic::{GenericFilter, GenericClause},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  schema::processes,
  models::{ColumnType, Pool, ProcessDb, ProcessUpdateDb},
};

use super::generic::*;

impl RepositoryBase for ProcessDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "processes.key")),
      ("name", (ColumnType::Text, "processes.name")),
      ("kind", (ColumnType::Text, "processes.kind")),
      ("node_name", (ColumnType::Text, "processes.node_name")),
      ("kind_key", (ColumnType::Text, "processes.kind_key")),
      ("data", (ColumnType::Json, "processes.data")),
      (
        "created_at",
        (ColumnType::Timestamptz, "processes.created_at"),
      ),
    ])
  }
}

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
    let mut query = diesel::delete(processes::table).into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns)
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
    let mut query = processes::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(processes::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for ProcessDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = processes::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
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
