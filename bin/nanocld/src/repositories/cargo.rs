use std::collections::HashMap;

use diesel::prelude::*;
use futures_util::{StreamExt, stream::FuturesUnordered};
use nanocl_error::{
  http::HttpResult,
  io::{IoError, IoResult},
};

use nanocl_stubs::{
  cargo::{Cargo, CargoDeleteQuery, CargoSummary},
  cargo_spec::{CargoSpec, CargoSpecRevision},
  generic::{GenericClause, GenericFilter, GenericFilterNsp},
  system::{ObjPsStatus, ObjPsStatusPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    CargoDb, CargoUpdateDb, ColumnType, NamespaceDb, ObjPsStatusDb, Pool,
    ProcessDb, ResourceIncarnationDb, SpecDb, SystemState,
  },
  objects::generic::*,
  schema::cargoes,
  utils,
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
      (
        "created_at",
        (ColumnType::Timestamptz, "cargoes.created_at"),
      ),
      ("updated_at", (ColumnType::Timestamptz, "specs.created_at")),
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
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(cargoes::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
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
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for CargoDb {
  type NewOutput = Cargo;

  fn transform(
    item: (CargoDb, SpecDb, ObjPsStatusDb),
  ) -> IoResult<Self::NewOutput> {
    let (cargo_db, spec_db, status) = item;
    let spec = spec_db.try_to_cargo_spec_revision()?;
    let item = cargo_db.with_spec(&(spec, status.try_into()?));
    Ok(item)
  }
}

impl WithSpec for CargoDb {
  type Output = Cargo;
  type Relation = (CargoSpecRevision, ObjPsStatus);

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
  /// Initial definition, specification and identity are committed together.
  pub(crate) async fn create_with_spec(
    key: &str,
    name: &str,
    namespace: &str,
    spec: SpecDb,
    status: ObjPsStatusPartial,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    Self::create_with_spec_checked(
      key, name, namespace, spec, status, false, pool,
    )
    .await
  }

  pub(crate) async fn create_bootstrap_with_spec(
    key: &str,
    name: &str,
    namespace: &str,
    spec: SpecDb,
    status: ObjPsStatusPartial,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    Self::create_with_spec_checked(
      key, name, namespace, spec, status, true, pool,
    )
    .await
  }

  async fn create_with_spec_checked(
    key: &str,
    name: &str,
    namespace: &str,
    mut spec: SpecDb,
    status: ObjPsStatusPartial,
    bootstrap: bool,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    use super::operation::{conflict, run_query_owned, transaction};
    use crate::schema::{
      object_process_statuses, resource_incarnations, specs,
    };
    if spec.kind_name != "Cargo"
      || spec.kind_key != key
      || spec.incarnation_key.is_some()
      || status.key != key
      || spec.try_to_cargo_spec()?.name != name
    {
      return Err(
        conflict("Initial Cargo definition has another identity").into(),
      );
    }
    let identity = ResourceIncarnationDb {
      key: uuid::Uuid::new_v4(),
      resource_kind: "Cargo".into(),
      resource_key: key.into(),
      created_at: chrono::Utc::now().naive_utc(),
      deleted_at: None,
      current_operation_key: None,
    };
    spec.incarnation_key = Some(identity.key);
    let status = ObjPsStatusDb::from(status);
    let cargo = CargoDb {
      key: key.into(),
      name: name.into(),
      created_at: identity.created_at,
      namespace_name: namespace.into(),
      status_key: key.into(),
      spec_key: spec.key,
      incarnation_key: identity.key,
    };
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        if bootstrap
          && resource_incarnations::table
            .filter(resource_incarnations::resource_kind.eq("Cargo"))
            .filter(resource_incarnations::resource_key.eq(&cargo.key))
            .select(resource_incarnations::key)
            .first::<uuid::Uuid>(conn)
            .optional()?
            .is_some()
        {
          return Err(conflict(
            "Retained Cargo identity prevents runtime reconstruction",
          ));
        }
        diesel::insert_into(resource_incarnations::table)
          .values(&identity)
          .execute(conn)?;
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let status = diesel::insert_into(object_process_statuses::table)
          .values(&status)
          .get_result::<ObjPsStatusDb>(conn)?;
        let cargo = diesel::insert_into(cargoes::table)
          .values(&cargo)
          .get_result::<CargoDb>(conn)?;
        Ok(Self::transform((cargo, spec, status))?)
      })
    })
    .await
  }

  pub(crate) async fn incarnation(
    key: &str,
    pool: &Pool,
  ) -> IoResult<uuid::Uuid> {
    Ok(Self::read_by_pk(key, pool).await?.0.incarnation_key)
  }

  pub(crate) async fn history(
    key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<CargoSpecRevision>> {
    let cargo = Self::read_by_pk(key, pool).await?.0;
    super::operation::run_query_owned(pool, (), move |conn| {
      use crate::{models::OperationTransactionError, schema::specs};
      specs::table
        .filter(specs::kind_name.eq("Cargo"))
        .filter(specs::kind_key.eq(&cargo.key))
        .filter(specs::incarnation_key.eq(cargo.incarnation_key))
        .order((specs::created_at.desc(), specs::key.desc()))
        .load::<SpecDb>(conn)
        .map_err(OperationTransactionError::from)?
        .into_iter()
        .map(|spec| spec.try_to_cargo_spec_revision())
        .collect()
    })
    .await
  }

  pub(crate) async fn history_spec(
    key: &str,
    revision: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<SpecDb> {
    let cargo = Self::read_by_pk(key, pool).await?.0;
    super::operation::run_query_owned(pool, (), move |conn| {
      use crate::{models::OperationTransactionError, schema::specs};
      specs::table
        .find(revision)
        .filter(specs::kind_name.eq("Cargo"))
        .filter(specs::kind_key.eq(&cargo.key))
        .filter(specs::incarnation_key.eq(cargo.incarnation_key))
        .first::<SpecDb>(conn)
        .optional()
        .map_err(OperationTransactionError::from)?
        .ok_or_else(|| {
          IoError::not_found(
            "Cargo history",
            "Revision does not belong to the current Cargo incarnation",
          )
        })
    })
    .await
  }

  /// Update a cargo from its specification.
  /// Bootstrap-only revision replacement; runtime actions use accept_operation.
  pub async fn update_from_spec(
    key: &str,
    item: &CargoSpec,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Cargo> {
    use super::operation::{
      conflict, lock_incarnation, run_query_owned, transaction,
    };
    use crate::schema::{object_process_statuses, specs};
    let expected = Self::read_by_pk(key, pool).await?.0;
    let mut spec = SpecDb::try_from_cargo_spec(key, version, item)?;
    spec.incarnation_key = Some(expected.incarnation_key);
    let name = item.name.clone();
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        let identity = lock_incarnation(conn, expected.incarnation_key)?;
        let cargo = cargoes::table
          .find(&expected.key)
          .for_update()
          .first::<CargoDb>(conn)?;
        let status = object_process_statuses::table
          .find(&cargo.status_key)
          .for_update()
          .first::<ObjPsStatusDb>(conn)?;
        if identity.deleted_at.is_some()
          || identity.resource_kind != "Cargo"
          || identity.resource_key != cargo.key
          || cargo.incarnation_key != expected.incarnation_key
          || cargo.spec_key != expected.spec_key
          || identity.current_operation_key.is_some()
          || super::cargo_operation::pending_phase(&status.actual)
        {
          return Err(conflict(
            "Cargo identity changed or has an unresolved pending transition",
          ));
        }
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let cargo = diesel::update(cargoes::table.find(&cargo.key))
          .set((cargoes::spec_key.eq(spec.key), cargoes::name.eq(&name)))
          .get_result::<CargoDb>(conn)?;
        Ok(Self::transform((cargo, spec, status))?)
      })
    })
    .await
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
    let filter = Self::collection_filter(query, state).await?;
    let cargoes =
      CargoDb::transform_read_by(&filter, &state.inner.pool).await?;
    let mut cargo_summaries = Vec::new();
    for cargo in cargoes {
      let spec = SpecDb::read_by_pk(&cargo.spec.key, &state.inner.pool)
        .await?
        .try_to_cargo_spec_revision()?;
      let filter = GenericFilter::new().r#where(
        "data",
        GenericClause::Contains(serde_json::json!({
          "Config": {
            "Labels": {
              "io.nanocl.not-init-c": "true"
            }
          }
        })),
      );
      let processes = ProcessDb::read_by_kind_key(
        &cargo.spec.cargo_key,
        Some(filter),
        &state.inner.pool,
      )
      .await?;
      let (_, _, _, running) =
        utils::container::generic::count_status(&processes);
      cargo_summaries.push(CargoSummary {
        created_at: cargo.created_at,
        status: cargo.status,
        namespace_name: cargo.namespace_name,
        instance_running: running,
        instance_total: processes.len(),
        spec: spec.clone(),
      });
    }
    Ok(cargo_summaries)
  }

  /// Count cargoes using the same namespace semantics as [`CargoDb::list`].
  pub async fn count(
    query: &GenericFilterNsp,
    state: &SystemState,
  ) -> HttpResult<i64> {
    let filter = Self::collection_filter(query, state).await?;
    Ok(CargoDb::count_by(&filter, &state.inner.pool).await?)
  }

  async fn collection_filter(
    query: &GenericFilterNsp,
    state: &SystemState,
  ) -> HttpResult<GenericFilter> {
    let mut filter = query.filter.clone().unwrap_or_default();
    if let Some(namespace) = utils::key::normalize_nsp(&query.namespace) {
      NamespaceDb::read_by_pk(namespace, &state.inner.pool).await?;
      filter = filter
        .r#where("namespace_name", GenericClause::Eq(namespace.to_owned()));
    }
    Ok(filter)
  }

  /// Delete an idle live definition while retaining its identity and revisions.
  pub async fn clear_by_pk(pk: &str, pool: &Pool) -> IoResult<()> {
    use super::operation::{
      conflict, lock_incarnation, run_query_owned, transaction,
    };
    use crate::schema::{
      object_process_statuses, operations, resource_incarnations,
    };
    let expected = Self::read_by_pk(pk, pool).await?.0;
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        let identity = lock_incarnation(conn, expected.incarnation_key)?;
        let Some(cargo) = cargoes::table
          .find(&expected.key)
          .for_update()
          .first::<CargoDb>(conn)
          .optional()?
        else {
          return Ok(());
        };
        if cargo.incarnation_key != expected.incarnation_key {
          return Err(conflict("Cargo name belongs to another incarnation"));
        }
        if let Some(key) = identity.current_operation_key
          && operations::table
            .find(key)
            .for_update()
            .first::<crate::models::OperationDb>(conn)?
            .outcome
            .is_none()
        {
          return Err(conflict(
            "An unresolved operation requires exact operation deletion",
          ));
        }
        diesel::delete(cargoes::table.find(&cargo.key)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&cargo.status_key))
          .execute(conn)?;
        diesel::update(resource_incarnations::table.find(identity.key))
          .set(
            resource_incarnations::deleted_at
              .eq(chrono::Utc::now().naive_utc()),
          )
          .execute(conn)?;
        Ok(())
      })
    })
    .await
  }
}
