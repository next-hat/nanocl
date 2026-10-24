use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, OperationOwner, Pool, ProcessDb, ProcessUpdateDb},
  schema::processes,
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
  pub(crate) async fn create_owned<G: Send + 'static>(
    item: &ProcessPartial,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<Self> {
    let item = Self::from(item);
    if owner.as_ref().is_some_and(|owner| {
      item.kind != owner.resource_kind
        || item.kind_key != owner.resource_key
        || item.node_name != owner.node_name
    }) {
      return Err(
        super::operation::conflict(
          "Process does not belong to its operation owner",
        )
        .into(),
      );
    }
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      Ok(
        diesel::insert_into(processes::table)
          .values(&item)
          .get_result(conn)?,
      )
    })
    .await
  }

  pub(crate) async fn delete_owned<G: Send + 'static>(
    key: &str,
    owner: Option<OperationOwner>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<()> {
    let key = key.to_owned();
    let expected_owner = owner.clone();
    super::operation::run_mutation_owned(pool, keepalive, owner, move |conn| {
      if let Some(owner) = &expected_owner
        && let Some(process) =
          processes::table.find(&key).first::<Self>(conn).optional()?
        && (process.kind != owner.resource_kind
          || process.kind_key != owner.resource_key
          || process.node_name != owner.node_name)
      {
        return Err(super::operation::conflict(
          "Process does not belong to its operation owner",
        ));
      }
      diesel::delete(processes::table.find(&key)).execute(conn)?;
      Ok(())
    })
    .await
  }

  /// Read network owners independently of the displayed page and its filters.
  pub async fn read_network_owners(
    targets: &[(String, String)],
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    if targets.is_empty() {
      return Ok(Vec::new());
    }
    let targets = targets.to_vec();
    let pool = pool.clone();
    ntex::rt::spawn_blocking(move || {
      let mut conn = crate::utils::store::get_pool_conn(&pool)?;
      let mut query = processes::table
        .filter(false.into_sql::<diesel::sql_types::Bool>())
        .into_boxed();
      for (node, target) in &targets {
        query = query.or_filter(
          processes::node_name
            .eq(node)
            .and(processes::key.eq(target).or(processes::name.eq(target))),
        );
      }
      query
        .load::<ProcessDb>(&mut conn)
        .map_err(Self::map_err)?
        .into_iter()
        .map(Process::try_from)
        .collect()
    })
    .await
    .map_err(|err| {
      IoError::interrupted("Read process network owners", &err.to_string())
    })?
  }

  pub async fn read_by_kind_key(
    kind_key: &str,
    filter: Option<GenericFilter>,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = filter
      .unwrap_or_default()
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
