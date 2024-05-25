use diesel::prelude::*;

use nanocl_error::{io::IoResult, http::HttpResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter, GenericFilterNsp},
  system::ObjPsStatus,
  vm::{Vm, VmSummary},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{
  gen_multiple, gen_where4json, gen_where4string,
  models::{
    NamespaceDb, ObjPsStatusDb, Pool, ProcessDb, SpecDb, VmDb, VmUpdateDb,
  },
  schema::vms,
  utils,
};

use super::generic::*;

impl RepositoryBase for VmDb {}

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
    let condition = filter.r#where.to_owned().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = vms::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, vms::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vms::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, vms::namespace_name, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, crate::schema::specs::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, crate::schema::specs::metadata, value);
    }
    if let Some(value) = r#where.get("status.wanted") {
      gen_where4string!(
        query,
        crate::schema::object_process_statuses::wanted,
        value
      );
    }
    if let Some(value) = r#where.get("status.actual") {
      gen_where4string!(
        query,
        crate::schema::object_process_statuses::actual,
        value
      );
    }
    if is_multiple {
      gen_multiple!(query, vms::created_at, filter);
    }
    query
  }
}

impl RepositoryCountBy for VmDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let condition = filter.r#where.to_owned().unwrap_or_default();
    let r#where = condition.conditions;
    let mut query = vms::table
      .inner_join(crate::schema::specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, vms::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vms::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, vms::namespace_name, value);
    }
    if let Some(value) = r#where.get("data") {
      gen_where4json!(query, crate::schema::specs::data, value);
    }
    if let Some(value) = r#where.get("metadata") {
      gen_where4json!(query, crate::schema::specs::metadata, value);
    }
    if let Some(value) = r#where.get("status.wanted") {
      gen_where4string!(
        query,
        crate::schema::object_process_statuses::wanted,
        value
      );
    }
    if let Some(value) = r#where.get("status.actual") {
      gen_where4string!(
        query,
        crate::schema::object_process_statuses::actual,
        value
      );
    }
    query.count()
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
  pub async fn update_from_spec(
    key: &str,
    item: &VmSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Vm> {
    let mut vm = VmDb::transform_read_by_pk(key, pool).await?;
    let new_spec = SpecDb::try_from_vm_partial(&vm.spec.vm_key, version, item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await?
      .try_to_vm_spec()?;
    let new_item = VmUpdateDb {
      name: Some(item.name.clone()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    VmDb::update_pk(key, new_item, pool).await?;
    vm.spec = spec;
    Ok(vm)
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
    let namespace = utils::key::resolve_nsp(&query.namespace);
    let namespace = NamespaceDb::read_by_pk(&namespace, pool).await?;
    let filter = query
      .filter
      .clone()
      .unwrap_or_default()
      .r#where("namespace_name", GenericClause::Eq(namespace.name.clone()));
    let vms = VmDb::transform_read_by(&filter, pool).await?;
    let mut vm_summaries = Vec::new();
    for vm in vms {
      let spec = SpecDb::read_by_pk(&vm.spec.key, pool)
        .await?
        .try_to_vm_spec()?;
      let processes =
        ProcessDb::read_by_kind_key(&vm.spec.vm_key, pool).await?;
      let (_, _, _, running_instances) =
        utils::container::count_status(&processes);
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

  pub async fn clear_by_pk(pk: &str, pool: &Pool) -> IoResult<()> {
    VmDb::del_by_pk(pk, pool).await?;
    SpecDb::del_by_kind_key(pk, pool).await?;
    ObjPsStatusDb::del_by_pk(pk, pool).await?;
    Ok(())
  }
}
