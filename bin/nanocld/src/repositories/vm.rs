use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::vm::Vm;
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::vm_config::{VmConfig, VmConfigPartial};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::utils;
use crate::models::{
  Pool, VmDbModel, NamespaceDbModel, VmConfigDbModel, VmUpdateDbModel,
};

use super::vm_config;

/// ## Find vm items by namespace
///
/// ## Arguments
///
/// - [nsp](NamespaceItem) - Namespace item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Vec<VmDbModel>) - List a vm found
///  - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::namespace::NamespaceItem;
/// let nsp = NamespaceItem {
///  name: String::from("test"),
/// };
/// let items = find_by_namespace(nsp, &pool).await;
/// ```
///
pub async fn find_by_namespace(
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

/// ## Create vm
///
/// Create a vm item in database for given namespace
///
/// ## Arguments
///
/// - [nsp](String) - Namespace name
/// - [item](VmPartial) - Vm item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vm) - The vm created
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::vm::VmConfigPartial;
///
/// let item = VmConfigPartial {
///   //... fill required data
///   name: String::from("test"),
///   container: bollard_next::container::Config {
///     image: Some(String::from("test")),
///     ..Default::default()
///   },
///   ..Default::default()
/// };
/// let vm = create(String::from("test"), item, &pool).await;
/// ```
///
pub async fn create(
  nsp: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Vm> {
  use crate::schema::vms::dsl;

  let nsp = nsp.to_owned();

  // test if the name of the vm include a . in the name and throw error if true
  if item.name.contains('.') {
    return Err(IoError::invalid_data(
      "VmConfigPartial",
      "Name cannot contain a dot.",
    ));
  }

  let pool = pool.clone();
  let key = utils::key::gen_key(&nsp, &item.name);

  let config = vm_config::create(&key, item, version, &pool).await?;

  let new_item = VmDbModel {
    key,
    name: item.name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    namespace_name: nsp,
    config_key: config.key,
  };

  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::vms)
      .values(&new_item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(new_item)
  })
  .await?;

  let vm = Vm {
    key: item.key,
    name: item.name,
    config_key: config.key,
    namespace_name: item.namespace_name,
    config,
  };

  Ok(vm)
}

/// ## Delete vm by key
///
/// Delete a vm item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Vm key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The number of deleted items
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let res = delete_by_key(String::from("test"), &pool).await;
/// ```
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::vms::dsl;

  let key = key.to_owned();
  let pool = pool.clone();
  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::delete(dsl::vms)
      .filter(dsl::key.eq(key))
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(res)
  })
  .await?;

  Ok(GenericDelete { count: res })
}

/// ## Find vm by key
///
/// Find a vm item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Vm key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmDbModel) - The vm found
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let vm = find_by_key(String::from("test"), &pool).await;
/// ```
///
pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<VmDbModel> {
  use crate::schema::vms::dsl;

  let key = key.to_owned();
  let pool = pool.clone();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::vms
      .filter(dsl::key.eq(key))
      .get_result(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(item)
  })
  .await?;

  Ok(item)
}

/// ## Update vm by key
///
/// Update a vm item in database for given key
///
/// ## Arguments
///
/// - [key](String) - Vm key
/// - [item](VmConfigPartial) - Vm config
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vm) - The vm updated
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::vm::VmConfigPartial;
/// let item = VmConfigPartial {
///  //... fill required data
///  name: String::from("test"),
///  container: bollard_next::container::Config {
///   image: Some(String::from("test")),
///   ..Default::default()
///  },
///  ..Default::default()
/// };
/// let vm = update_by_key(String::from("test"), item, &pool).await;
/// ```
///
pub async fn update_by_key(
  key: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<Vm> {
  use crate::schema::vms::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let vmdb = find_by_key(&key, &pool).await?;
  let config = vm_config::create(&key, item, version, &pool).await?;

  let new_item = VmUpdateDbModel {
    name: Some(item.name.clone()),
    config_key: Some(config.key),
    ..Default::default()
  };

  web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::update(dsl::vms.filter(dsl::key.eq(key)))
      .set(&new_item)
      .execute(&mut conn)
      .map_err(|err| err.map_err_context(|| "Vm"))?;
    Ok::<_, IoError>(())
  })
  .await?;

  let vm = Vm {
    key: vmdb.key,
    name: vmdb.name,
    config_key: config.key,
    namespace_name: vmdb.namespace_name,
    config,
  };

  Ok(vm)
}

pub async fn inspect_by_key(key: &str, pool: &Pool) -> IoResult<Vm> {
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

  let config = serde_json::from_value::<VmConfigPartial>(item.1.config)
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
    host_config: config.host_config,
    password: config.password,
    ssh_key: config.ssh_key,
  };

  let item = Vm {
    key: item.0.key,
    name: item.0.name,
    config_key: item.1.key,
    namespace_name: item.0.namespace_name,
    config,
  };

  Ok(item)
}
