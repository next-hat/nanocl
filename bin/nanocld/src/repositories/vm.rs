use std::collections::HashMap;

use diesel::prelude::*;

use nanocl_error::{http::HttpResult, io::IoResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter, GenericFilterNsp},
  system::{ObjPsStatus, ObjPsStatusPartial},
  vm::{Vm, VmSummary},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    ColumnType, NamespaceDb, ObjPsStatusDb, OperationDb, Pool, ProcessDb,
    ResourceIncarnationDb, SpecDb, VmDb, VmUpdateDb,
  },
  schema::{
    object_process_statuses, operations, resource_incarnations, specs, vms,
  },
  utils,
};

use super::generic::*;

impl RepositoryBase for VmDb {
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "vms.key")),
      ("created_at", (ColumnType::Timestamptz, "vms.created_at")),
      ("name", (ColumnType::Text, "vms.name")),
      ("namespace_name", (ColumnType::Text, "vms.namespace_name")),
      ("spec_key", (ColumnType::Text, "vms.spec_key")),
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

impl RepositoryCreate for VmDb {}

impl RepositoryUpdate for VmDb {
  type UpdateItem = VmUpdateDb;
}

impl RepositoryDelByPk for VmDb {}

impl RepositoryReadBy for VmDb {
  type Output = (VmDb, SpecDb, ObjPsStatusDb);

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
  > {
    let mut query = vms::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(vms::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for VmDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = vms::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for VmDb {
  type NewOutput = Vm;

  fn transform(
    input: (VmDb, SpecDb, ObjPsStatusDb),
  ) -> IoResult<Self::NewOutput> {
    let spec = input.1.try_to_vm_spec()?;
    let item = input.0.with_spec(&(spec, input.2.try_into()?));
    Ok(item)
  }
}

impl WithSpec for VmDb {
  type Output = Vm;
  type Relation = (VmSpec, ObjPsStatus);

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.0.clone(),
      status: r.1.clone(),
    }
  }
}

impl VmDb {
  pub(crate) async fn create_with_spec(
    key: &str,
    name: &str,
    namespace: &str,
    mut spec: SpecDb,
    status: ObjPsStatusPartial,
    pool: &Pool,
  ) -> IoResult<Vm> {
    use super::operation::{conflict, run_query_owned, transaction};
    let identity = ResourceIncarnationDb {
      key: uuid::Uuid::new_v4(),
      resource_kind: "Vm".into(),
      resource_key: key.into(),
      created_at: chrono::Utc::now().naive_utc(),
      deleted_at: None,
      current_operation_key: None,
    };
    if spec.kind_name != "Vm"
      || spec.kind_key != key
      || spec.incarnation_key.is_some()
      || status.key != key
    {
      return Err(
        conflict("Initial VM specification or status has another identity")
          .into(),
      );
    }
    spec.incarnation_key = Some(identity.key);
    let status = ObjPsStatusDb::from(status);
    let vm = VmDb {
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
        diesel::insert_into(resource_incarnations::table)
          .values(&identity)
          .execute(conn)?;
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let status = diesel::insert_into(object_process_statuses::table)
          .values(&status)
          .get_result::<ObjPsStatusDb>(conn)?;
        let vm = diesel::insert_into(vms::table)
          .values(&vm)
          .get_result::<VmDb>(conn)?;
        Ok(Self::transform((vm, spec, status))?)
      })
    })
    .await
  }

  pub(crate) async fn history(key: &str, pool: &Pool) -> IoResult<Vec<SpecDb>> {
    let vm = Self::read_by_pk(key, pool).await?.0;
    super::operation::run_query_owned(pool, (), move |conn| {
      Ok(
        specs::table
          .filter(specs::kind_name.eq("Vm"))
          .filter(specs::incarnation_key.eq(vm.incarnation_key))
          .order(specs::created_at.desc())
          .load(conn)
          .map_err(crate::models::OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub async fn update_from_spec(
    key: &str,
    item: &VmSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Vm> {
    Self::update_from_spec_owned(key, item, version, (), pool).await
  }

  pub(crate) async fn update_from_spec_owned<G: Send + 'static>(
    key: &str,
    item: &VmSpecPartial,
    version: &str,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<Vm> {
    use super::operation::{
      conflict, lock_incarnation, run_query_owned, transaction,
    };
    let expected = Self::read_by_pk(key, pool).await?.0;
    let mut spec = SpecDb::try_from_vm_partial(key, version, item)?;
    spec.incarnation_key = Some(expected.incarnation_key);
    let name = item.name.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let identity = lock_incarnation(conn, expected.incarnation_key)?;
        let vm = vms::table
          .find(&expected.key)
          .for_update()
          .first::<VmDb>(conn)?;
        if identity.deleted_at.is_some()
          || vm.incarnation_key != expected.incarnation_key
          || vm.spec_key != expected.spec_key
        {
          return Err(conflict(
            "VM incarnation or specification changed during revision update",
          ));
        }
        let spec = diesel::insert_into(specs::table)
          .values(&spec)
          .get_result::<SpecDb>(conn)?;
        let vm = diesel::update(vms::table.find(&vm.key))
          .set((vms::spec_key.eq(spec.key), vms::name.eq(&name)))
          .get_result::<VmDb>(conn)?;
        let status = object_process_statuses::table
          .find(&vm.status_key)
          .first::<ObjPsStatusDb>(conn)?;
        Ok(Self::transform((vm, spec, status))?)
      })
    })
    .await
  }

  pub async fn read_by_namespace(name: &str, pool: &Pool) -> IoResult<Vec<Vm>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    VmDb::transform_read_by(&filter, pool).await
  }

  /// List VMs by namespace
  pub async fn list(
    query: &GenericFilterNsp,
    pool: &Pool,
  ) -> HttpResult<Vec<VmSummary>> {
    let filter = Self::collection_filter(query, pool).await?;
    let vms = VmDb::transform_read_by(&filter, pool).await?;
    let mut vm_summaries = Vec::new();
    for vm in vms {
      let spec = SpecDb::read_by_pk(&vm.spec.key, pool)
        .await?
        .try_to_vm_spec()?;
      let processes =
        ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, pool).await?;
      let (_, _, _, running_instances) =
        utils::container::generic::count_status(&processes);
      vm_summaries.push(VmSummary {
        created_at: vm.created_at,
        status: vm.status,
        namespace_name: vm.namespace_name,
        instance_total: processes.len(),
        instance_running: running_instances,
        spec: spec.clone(),
      });
    }
    Ok(vm_summaries)
  }

  /// Count VMs using the same namespace semantics as [`VmDb::list`].
  pub async fn count(query: &GenericFilterNsp, pool: &Pool) -> HttpResult<i64> {
    let filter = Self::collection_filter(query, pool).await?;
    Ok(VmDb::count_by(&filter, pool).await?)
  }

  async fn collection_filter(
    query: &GenericFilterNsp,
    pool: &Pool,
  ) -> HttpResult<GenericFilter> {
    let mut filter = query.filter.clone().unwrap_or_default();
    if let Some(namespace) = utils::key::normalize_nsp(&query.namespace) {
      NamespaceDb::read_by_pk(namespace, pool).await?;
      filter = filter
        .r#where("namespace_name", GenericClause::Eq(namespace.to_owned()));
    }
    Ok(filter)
  }

  pub async fn clear_by_pk(pk: &str, pool: &Pool) -> IoResult<()> {
    Self::clear_by_pk_owned(pk, (), pool).await
  }

  pub(crate) async fn clear_by_pk_owned<G: Send + 'static>(
    pk: &str,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<()> {
    use super::operation::{
      conflict, lock_incarnation, run_query_owned, transaction,
    };
    let expected = Self::read_by_pk(pk, pool).await?.0;
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let identity = lock_incarnation(conn, expected.incarnation_key)?;
        let Some(vm) = vms::table
          .find(&expected.key)
          .for_update()
          .first::<VmDb>(conn)
          .optional()?
        else {
          return Ok(());
        };
        if vm.incarnation_key != expected.incarnation_key {
          return Err(conflict("VM name belongs to a newer incarnation"));
        }
        if let Some(key) = identity.current_operation_key
          && operations::table
            .find(key)
            .for_update()
            .first::<OperationDb>(conn)?
            .outcome
            .is_none()
        {
          return Err(conflict(
            "An unresolved operation requires exact operation deletion",
          ));
        }
        diesel::delete(vms::table.find(&vm.key)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&vm.status_key))
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
