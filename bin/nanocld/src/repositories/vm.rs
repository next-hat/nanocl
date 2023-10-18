use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::{generic, vm, vm_config};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::{utils, schema, models};

/// ## Find by namespace
///
/// Find a vm by a `models::NamespaceDbModel` in database and return a `Vec<models::VmDbModel>`
///
/// ## Arguments
///
/// - [nsp](models::NamespaceDbModel) - Namespace item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<models::VmDbModel>) - List a vm found
///  - [Err](IoError) - Error during the operation
///
pub async fn find_by_namespace(
  nsp: &models::NamespaceDbModel,
  pool: &models::Pool,
) -> IoResult<Vec<models::VmDbModel>> {
  let nsp = nsp.clone();
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = models::VmDbModel::belonging_to(&nsp)
      .load(&mut conn)
      .map_err(|err| err.map_err_context(|| "vm::Vm"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Create
///
/// Create a vm item in database for given namespace
/// from a `vm_config::VmConfigPartial` and return a `vm::Vm`.
///
/// ## Arguments
///
/// - [nsp](str) - Namespace name
/// - [item](vm_config::VmConfigPartial) - vm::Vm item
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](vm::Vm) - The vm created
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  nsp: &str,
  item: &vm_config::VmConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> IoResult<vm::Vm> {
  let nsp = nsp.to_owned();
  // test if the name of the vm include a . in the name and throw error if true
  if item.name.contains('.') {
    return Err(IoError::invalid_data(
      "vm_config::VmConfigPartial",
      "Name cannot contain a dot.",
    ));
  }
  let key = utils::key::gen_key(&nsp, &item.name);
  let config = super::vm_config::create(&key, item, version, pool).await?;
  let new_item = models::VmDbModel {
    key,
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };

  let item: models::VmDbModel =
    utils::repository::generic_insert_with_res(pool, new_item).await?;

  let vm = item.into_vm(config);
  Ok(vm)
}

/// ## Delete by key
///
/// Delete a vm item in database for given key
///
/// ## Arguments
///
/// - [key](str) - vm::Vm key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericDelete) - The number of deleted items
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_key(
  key: &str,
  pool: &models::Pool,
) -> IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete_by_id::<schema::vms::table, _>(pool, key)
    .await
}

/// ## Find by key
///
/// Find a vm item in database for given key
///
/// ## Arguments
///
/// - [key](str) - vm::Vm key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::VmDbModel) - The vm found
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &str,
  pool: &models::Pool,
) -> IoResult<models::VmDbModel> {
  let key = key.to_owned();

  utils::repository::generic_find_by_id::<schema::vms::table, _, _>(pool, key)
    .await
}

/// ## Update by key
///
/// Update a vm item in database for given key
///
/// ## Arguments
///
/// - [key](str) - vm::Vm key
/// - [item](vm_config::VmConfigPartial) - vm::Vm config
/// - [version](str) - vm::Vm version
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](vm::Vm) - The vm updated
///   - [Err](IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &vm_config::VmConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> IoResult<vm::Vm> {
  let key = key.to_owned();
  let vmdb = find_by_key(&key, pool).await?;
  let config = super::vm_config::create(&key, item, version, pool).await?;
  let new_item = models::VmUpdateDbModel {
    name: Some(item.name.clone()),
    config_key: Some(config.key),
    ..Default::default()
  };

  utils::repository::generic_update_by_id::<
    schema::vms::table,
    models::VmUpdateDbModel,
    _,
  >(pool, key, new_item)
  .await?;

  let vm = vmdb.into_vm(config);
  Ok(vm)
}

/// ## Inspect by key
///
/// Inspect a vm item in database for given key and return a `vm::Vm`.
///
/// ## Arguments
///
/// - [key](str) - vm::Vm key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](vm::Vm) - The vm found
///   - [Err](IoError) - Error during the operation
///
pub async fn inspect_by_key(
  key: &str,
  pool: &models::Pool,
) -> IoResult<vm::Vm> {
  use crate::schema::vms;
  use crate::schema::vm_configs;
  let key = key.to_owned();
  let pool = pool.clone();
  let item: (models::VmDbModel, models::VmConfigDbModel) =
    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = vms::table
        .inner_join(vm_configs::table)
        .filter(vms::key.eq(key))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "vm::Vm"))?;
      Ok::<_, IoError>(item)
    })
    .await?;
  let config =
    serde_json::from_value::<vm_config::VmConfigPartial>(item.1.config)
      .map_err(|err| err.map_err_context(|| "vm_config::VmConfigPartial"))?;
  let config = vm_config::VmConfig {
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
