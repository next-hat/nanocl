use std::sync::Arc;

use ntex::rt::JoinHandle;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  vm::Vm,
  vm_spec::{VmSpecPartial, VmSpec},
};

use crate::{
  utils, gen_where4string,
  models::{Pool, VmDb, VmUpdateDb, SpecDb},
  schema::{vms, specs},
};

use super::generic::*;

impl RepositoryBase for VmDb {}

impl RepositoryCreate for VmDb {}

impl RepositoryUpdate for VmDb {
  type UpdateItem = VmUpdateDb;
}

impl RepositoryDelByPk for VmDb {}

impl RepositoryReadWithSpec for VmDb {
  type Output = Vm;

  fn read_pk_with_spec(
    pk: &str,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("VmDb::find_by_pk: {pk}");
    let pool = Arc::clone(pool);
    let pk = pk.to_owned();
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = vms::table
        .inner_join(specs::table)
        .filter(vms::key.eq(pk))
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1.try_to_vm_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn read_one_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Output>> {
    log::trace!("VmDb::find_one: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vms::table.inner_join(specs::table).into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, vms::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, vms::name, value);
    }
    if let Some(value) = r#where.get("namespace_name") {
      gen_where4string!(query, vms::namespace_name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let item = item.0.with_spec(&item.1.try_to_vm_spec()?);
      Ok::<_, IoError>(item)
    })
  }

  fn read_with_spec(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Output>>> {
    log::trace!("VmDb::find: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = vms::table
      .inner_join(specs::table)
      .order(vms::created_at.desc())
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
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<(Self, SpecDb)>(&mut conn)
        .map_err(Self::map_err)?;
      let items = items
        .into_iter()
        .map(|item| {
          let spec = &item.1.try_to_vm_spec()?;
          Ok::<_, IoError>(item.0.with_spec(spec))
        })
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
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
    let mut vm = VmDb::read_pk_with_spec(key, pool).await??;
    let new_spec = SpecDb::try_from_vm_partial(&vm.spec.vm_key, version, item)?;
    let spec = SpecDb::create_from(new_spec, pool)
      .await?
      .try_to_vm_spec()?;
    let new_item = VmUpdateDb {
      name: Some(item.name.clone()),
      spec_key: Some(spec.key),
      ..Default::default()
    };
    VmDb::update_pk(key, new_item, pool).await??;
    vm.spec = spec;
    Ok(vm)
  }

  pub(crate) async fn inspect_by_pk(pk: &str, pool: &Pool) -> IoResult<Vm> {
    VmDb::read_pk_with_spec(pk, pool).await?
  }

  pub(crate) async fn find_by_namespace(
    name: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Vm>> {
    let filter = GenericFilter::new()
      .r#where("namespace_name", GenericClause::Eq(name.to_owned()));
    VmDb::read_with_spec(&filter, pool).await?
  }
}
