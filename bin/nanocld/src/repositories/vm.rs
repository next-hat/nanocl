use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, FromIo, IoResult};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::vm::Vm;
use nanocl_stubs::vm_config::{VmConfig, VmConfigPartial};

use crate::utils;
use crate::models::{
  Pool, VmDbModel, VmUpdateDbModel, VmConfigDbModel, NamespaceDbModel,
};

/// ## Find by namespace
///
/// Find a vm by a `NamespaceDbModel` in database and return a `Vec<VmDbModel>`
///
/// ## Arguments
///
/// * [nsp](NamespaceDbModel) - Namespace item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [VmDbModel](VmDbModel)
///
pub(crate) async fn find_by_namespace(
  nsp: &NamespaceDbModel,
  pool: &Pool,
) -> IoResult<Vec<VmDbModel>> {
  let nsp = nsp.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = VmDbModel::belonging_to(&nsp)
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
/// from a `VmConfigPartial` and return a `Vm`.
///
/// ## Arguments
///
/// * [nsp](str) - Namespace name
/// * [item](VmConfigPartial) - Vm item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vm](Vm)
///
pub(crate) async fn create(
  nsp: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Vm> {
  let nsp = nsp.to_owned();
  if item.name.contains('.') {
    return Err(IoError::invalid_data(
      "VmConfigPartial",
      "Name cannot contain a dot.",
    ));
  }
  let key = utils::key::gen_key(&nsp, &item.name);
  let config = super::vm_config::create(&key, item, version, pool).await?;
  let new_item = VmDbModel {
    key,
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };
  let item: VmDbModel = super::generic::insert_with_res(new_item, pool).await?;
  let vm = item.into_vm(config);
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
/// [IoResult](IoResult) containing a [VmDbModel](VmDbModel)
///
pub(crate) async fn find_by_key(key: &str, pool: &Pool) -> IoResult<VmDbModel> {
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
/// * [item](VmConfigPartial) - Vm config
/// * [version](str) - Vm version
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vm](Vm)
///
pub(crate) async fn update_by_key(
  key: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Vm> {
  use crate::schema::vms;
  let key = key.to_owned();
  let vmdb = find_by_key(&key, pool).await?;
  let config = super::vm_config::create(&key, item, version, pool).await?;
  let new_item = VmUpdateDbModel {
    name: Some(item.name.clone()),
    config_key: Some(config.key),
    ..Default::default()
  };
  super::generic::update_by_id::<vms::table, VmUpdateDbModel, _>(
    key, new_item, pool,
  )
  .await?;
  let vm = vmdb.into_vm(config);
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
  use crate::schema::vm_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let item: (VmDbModel, VmConfigDbModel) = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = vms::table
      .inner_join(vm_configs::table)
      .filter(vms::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  let config = serde_json::from_value::<VmConfigPartial>(item.1.data)
    .map_err(|err| err.map_err_context(|| "VmConfigPartial"))?;
  let config = VmConfig {
    key: item.1.key,
    created_at: item.0.created_at,
    name: config.name,
    version: item.1.version,
    vm_key: item.1.vm_key,
    hostname: config.hostname,
    disk: config.disk,
    user: config.user,
    mac_address: config.mac_address,
    labels: config.labels,
    host_config: config.host_config.unwrap_or_default(),
    password: config.password,
    ssh_key: config.ssh_key,
    metadata: config.metadata,
  };
  let item = item.0.into_vm(config);
  Ok(item)
}
