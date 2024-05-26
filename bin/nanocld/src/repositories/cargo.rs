use std::collections::HashMap;

use diesel::prelude::*;
use futures_util::{StreamExt, stream::FuturesUnordered};
use nanocl_error::{
  http::HttpResult,
  io::{IoError, IoResult},
};

use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoSummary},
  cargo_spec::{CargoSpec, CargoSpecPartial},
  generic::{GenericClause, GenericFilter, GenericFilterNsp},
  system::ObjPsStatus,
};

use crate::{
  gen_sql_order_by, gen_sql_multiple, gen_sql_query, utils,
  schema::cargoes,
  objects::generic::*,
  models::{
    CargoDb, CargoUpdateDb, ColumnType, NamespaceDb, ObjPsStatusDb, Pool,
    ProcessDb, SpecDb, SystemState,
  },
};

use super::generic::*;

impl RepositoryBase for CargoDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "cargoes.key")),
      ("name", (ColumnType::Text, "cargoes.name")),
      (
        "namespace_name",
        (ColumnType::Text, "cargoes.namespace_name"),
      ),
      ("data", (ColumnType::Json, "specs.data")),
      ("metadata", (ColumnType::Json, "specs.metadata")),
      (
        "status.wanted",
        (ColumnType::Text, "object_process_statuses.wanted"),
      ),
      (
        "status.actual",
        (ColumnType::Text, "object_process_statuses.actual"),
      ),
    ])
  }
}

impl RepositoryCreate for CargoDb {}

impl RepositoryUpdate for CargoDb {
  type UpdateItem = CargoUpdateDb;
}

impl RepositoryDelByPk for CargoDb {}

impl RepositoryReadBy for CargoDb {
  type Output = (CargoDb, SpecDb, ObjPsStatusDb);

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  >
  where
    Self::Output: Sized,
  {
    let mut query = cargoes::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(cargoes::table, query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    }
    if is_multiple {
      gen_sql_multiple!(query, cargoes::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for CargoDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = cargoes::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(cargoes::table, query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for CargoDb {
  type NewOutput = Cargo;

  fn transform(
    item: (CargoDb, SpecDb, ObjPsStatusDb),
  ) -> IoResult<Self::NewOutput> {
    let (cargo_db, spec_db, status) = item;
    let spec = spec_db.try_to_cargo_spec()?;
    let item = cargo_db.with_spec(&(spec, status.try_into()?));
    Ok(item)
  }
}

impl WithSpec for CargoDb {
  type Output = Cargo;
  type Relation = (CargoSpec, ObjPsStatus);

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.0.clone(),
      status: r.1.clone(),
    }
  }
}

impl CargoDb {
  /// Update a cargo from its specification.
  pub async fn update_from_spec(
    key: &str,
    item: &CargoSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    let version = version.to_owned();
    let mut cargo = CargoDb::transform_read_by_pk(key, pool).await?;
    let new_spec = SpecDb::try_from_cargo_partial(key, &version, item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await?
      .try_to_cargo_spec()?;
    let new_item = CargoUpdateDb {
      name: Some(item.name.to_owned()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    CargoDb::update_pk(key, new_item, pool).await?;
    cargo.spec = spec;
    Ok(cargo)
  }

  /// Find cargoes by namespace.
  pub async fn read_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Cargo>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    CargoDb::transform_read_by(&filter, pool).await
  }

  /// Count cargoes by namespace.
  pub async fn count_by_namespace(nsp: &str, pool: &Pool) -> IoResult<i64> {
    let nsp = nsp.to_owned();
    let pool = pool.clone();
    let count = ntex::web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let count = cargoes::table
        .filter(cargoes::namespace_name.eq(nsp))
        .count()
        .get_result(&mut conn)
        .map_err(Self::map_err)?;
      Ok::<_, IoError>(count)
    })
    .await?;
    Ok(count)
  }

  /// This remove all cargo in the given namespace and all their instances (containers)
  /// from the system (database and docker).
  pub async fn delete_by_namespace(
    namespace: &str,
    state: &SystemState,
  ) -> HttpResult<()> {
    let namespace =
      NamespaceDb::read_by_pk(namespace, &state.inner.pool).await?;
    let cargoes =
      CargoDb::read_by_namespace(&namespace.name, &state.inner.pool).await?;
    cargoes
      .into_iter()
      .map(|cargo| async move {
        CargoDb::del_obj_by_pk(
          &cargo.spec.cargo_key,
          &CargoDeleteQuery::default(),
          state,
        )
        .await
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<_>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
    Ok(())
  }

  /// List the cargoes for the given query
  pub async fn list(
    query: &GenericFilterNsp,
    state: &SystemState,
  ) -> HttpResult<Vec<CargoSummary>> {
    let namespace = utils::key::resolve_nsp(&query.namespace);
    let filter = query
      .filter
      .clone()
      .unwrap_or_default()
      .r#where("namespace_name", GenericClause::Eq(namespace.clone()));
    NamespaceDb::read_by_pk(&namespace, &state.inner.pool).await?;
    let cargoes =
      CargoDb::transform_read_by(&filter, &state.inner.pool).await?;
    let mut cargo_summaries = Vec::new();
    for cargo in cargoes {
      let spec = SpecDb::read_by_pk(&cargo.spec.key, &state.inner.pool)
        .await?
        .try_to_cargo_spec()?;
      let processes =
        ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.inner.pool)
          .await?;
      let (_, _, _, running) = utils::container::count_status(&processes);
      cargo_summaries.push(CargoSummary {
        created_at: cargo.created_at,
        status: cargo.status,
        namespace_name: cargo.namespace_name,
        instance_total: processes.len(),
        instance_running: running,
        spec: spec.clone(),
      });
    }
    Ok(cargo_summaries)
  }

  /// Delete a cargo and it's relations (Spec, ObjPsStatus).
  pub async fn clear_by_pk(pk: &str, pool: &Pool) -> IoResult<()> {
    CargoDb::del_by_pk(pk, pool).await?;
    SpecDb::del_by_kind_key(pk, pool).await?;
    ObjPsStatusDb::del_by_pk(pk, pool).await?;
    Ok(())
  }
}
