use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  vm::Vm,
  vm_spec::{VmSpecPartial, VmSpec},
};

use crate::{
  gen_multiple, gen_where4string, utils,
  schema::vms,
  models::{Pool, VmDb, VmUpdateDb, SpecDb},
};

use super::generic::*;

impl RepositoryBase for VmDb {}

impl RepositoryCreate for VmDb {}

impl RepositoryUpdate for VmDb {
  type UpdateItem = VmUpdateDb;
}

impl RepositoryDelByPk for VmDb {}

impl RepositoryReadBy for VmDb {
  type Output = (VmDb, SpecDb);

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

  fn transform(input: (VmDb, SpecDb)) -> IoResult<Self::NewOutput> {
    let spec = input.1.try_to_vm_spec()?;
    let item = input.0.with_spec(&spec);
    Ok(item)
  }
}

impl WithSpec for VmDb {
  type Output = Vm;
  type Relation = VmSpec;

  fn with_spec(self, r: &Self::Relation) -> Self::Output {
    Self::Output {
      namespace_name: self.namespace_name,
      created_at: self.created_at,
      spec: r.clone(),
    }
  }
}

impl VmDb {
  pub(crate) async fn create_from_spec(
    nsp: &str,
    item: &VmSpecPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Vm> {
    let nsp = nsp.to_owned();
    if item.name.contains('.') {
      return Err(IoError::invalid_data(
        "VmSpecPartial",
        "Name cannot contain a dot.",
      ));
    }
    let key = utils::key::gen_key(&nsp, &item.name);
    let new_spec = SpecDb::try_from_vm_partial(&key, version, item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await?
      .try_to_vm_spec()?;
    let new_item = VmDb {
      key,
      name: item.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: nsp,
      spec_key: spec.key,
    };
    let item = VmDb::create_from(new_item, pool).await?;
    let vm = item.with_spec(&spec);
    Ok(vm)
  }

  pub(crate) async fn update_from_spec(
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

  pub(crate) async fn read_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Vm>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    VmDb::transform_read_by(&filter, pool).await
  }
}
