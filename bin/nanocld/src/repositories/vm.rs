use diesel::prelude::*;

use nanocl_error::{io::IoResult, http::HttpResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  system::ObjPsStatus,
  vm::{Vm, VmSummary},
  vm_spec::{VmSpec, VmSpecPartial},
};

use crate::{
  utils,
  schema::vms,
  gen_multiple, gen_where4string,
  models::{
    NamespaceDb, ObjPsStatusDb, Pool, ProcessDb, SpecDb, VmDb, VmUpdateDb,
  },
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
    let r#where = filter.r#where.to_owned().unwrap_or_default();
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
    if is_multiple {
      gen_multiple!(query, vms::created_at, filter);
    }
    query
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
  pub async fn list_by_namespace(
    nsp: &str,
    pool: &Pool,
  ) -> HttpResult<Vec<VmSummary>> {
    let namespace = NamespaceDb::read_by_pk(nsp, pool).await?;
    let vmes = VmDb::read_by_namespace(&namespace.name, pool).await?;
    let mut vm_summaries = Vec::new();
    for vm in vmes {
      let spec = SpecDb::read_by_pk(&vm.spec.key, pool)
        .await?
        .try_to_vm_spec()?;
      let processes =
        ProcessDb::read_by_kind_key(&vm.spec.vm_key, pool).await?;
      let (_, _, _, running_instances) =
        utils::container::count_status(&processes);
      vm_summaries.push(VmSummary {
        created_at: vm.created_at,
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
