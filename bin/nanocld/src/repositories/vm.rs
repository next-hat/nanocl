use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, FromIo, IoResult};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::vm::Vm;
use nanocl_stubs::vm_spec::{VmSpec, VmSpecPartial};

use crate::utils;
use crate::models::{Pool, VmDb, VmUpdateDb, VmSpecDb, NamespaceDb};

/// ## Find by namespace
///
/// Find a vm by a `NamespaceDb` in database and return a `Vec<VmDb>`
///
/// ## Arguments
///
/// * [nsp](NamespaceDb) - Namespace item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [VmDb](VmDb)
///
pub(crate) async fn find_by_namespace(
  nsp: &NamespaceDb,
  pool: &Pool,
) -> IoResult<Vec<VmDb>> {
  let nsp = nsp.clone();
  let pool = Arc::clone(pool);
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = VmDb::belonging_to(&nsp)
      .load(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Create
///
/// Create a vm item in database for given namespace
/// from a `VmSpecPartial` and return a `Vm`.
///
/// ## Arguments
///
/// * [nsp](str) - Namespace name
/// * [item](VmSpecPartial) - Vm item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vm](Vm)
///
pub(crate) async fn create(
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
  let spec = super::vm_spec::create(&key, item, version, pool).await?;
  let new_item = VmDb {
    key,
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    spec_key: spec.key,
  };
  let item: VmDb = super::generic::insert_with_res(new_item, pool).await?;
  let vm = item.into_vm(spec);
  Ok(vm)
}

/// ## Delete by key
///
/// Delete a vm item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Vm key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::vms;
  let key = key.to_owned();
  super::generic::delete_by_id::<vms::table, _>(key, pool).await
}

/// ## Find by key
///
/// Find a vm item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Vm key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [VmDb](VmDb)
///
pub(crate) async fn find_by_key(key: &str, pool: &Pool) -> IoResult<VmDb> {
  use crate::schema::vms;
  let key = key.to_owned();
  super::generic::find_by_id::<vms::table, _, _>(key, pool).await
}

/// ## Update by key
///
/// Update a vm item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Vm key
/// * [item](VmSpecPartial) - Vm spec
/// * [version](str) - Vm version
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vm](Vm)
///
pub(crate) async fn update_by_key(
  key: &str,
  item: &VmSpecPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Vm> {
  use crate::schema::vms;
  let key = key.to_owned();
  let vmdb = find_by_key(&key, pool).await?;
  let spec = super::vm_spec::create(&key, item, version, pool).await?;
  let new_item = VmUpdateDb {
    name: Some(item.name.clone()),
    spec_key: Some(spec.key),
    ..Default::default()
  };
  super::generic::update_by_id::<vms::table, VmUpdateDb, _>(
    key, new_item, pool,
  )
  .await?;
  let vm = vmdb.into_vm(spec);
  Ok(vm)
}

/// ## Inspect by key
///
/// Inspect a vm item in database for given key and return a `Vm`.
///
/// ## Arguments
///
/// * [key](str) - Vm key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vm](Vm)
///
pub(crate) async fn inspect_by_key(key: &str, pool: &Pool) -> IoResult<Vm> {
  use crate::schema::vms;
  use crate::schema::vm_specs;
  let key = key.to_owned();
  let pool = Arc::clone(pool);
  let item: (VmDb, VmSpecDb) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = vms::table
      .inner_join(vm_specs::table)
      .filter(vms::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  let spec = serde_json::from_value::<VmSpecPartial>(item.1.data)
    .map_err(|err| err.map_err_context(|| "VmSpecPartial"))?;
  let spec = VmSpec {
    key: item.1.key,
    created_at: item.0.created_at,
    name: spec.name,
    version: item.1.version,
    vm_key: item.1.vm_key,
    hostname: spec.hostname,
    disk: spec.disk,
    user: spec.user,
    mac_address: spec.mac_address,
    labels: spec.labels,
    host_config: spec.host_config.unwrap_or_default(),
    password: spec.password,
    ssh_key: spec.ssh_key,
    metadata: spec.metadata,
  };
  let item = item.0.into_vm(spec);
  Ok(item)
}
