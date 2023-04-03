use ntex::web;
use ntex::http::StatusCode;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::vm_config::{VmConfig, VmConfigPartial};

use crate::utils;
use crate::error::HttpError;
use crate::models::{Pool, VmConfigDbModel};
use super::error::{db_error, db_blocking_error};

/// ## Create vm config
///
/// Create a vm config item in database for given vm
///
/// ## Arguments
///
/// - [vm_key](String) - Vm key
/// - [item](VmConfigPartial) - Vm config item
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmConfig) - The created vm config
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// use nanocl_stubs::vm_config::VmConfigPartial;
///
/// let item = VmConfigPartial {
///  // Fill config
/// };
/// let config = create("test".into(), item, &pool).await;
/// ```
///
pub async fn create(
  vm_key: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> Result<VmConfig, HttpError> {
  use crate::schema::vm_configs::dsl;

  let pool = pool.clone();
  let dbmodel = VmConfigDbModel {
    key: uuid::Uuid::new_v4(),
    vm_key: vm_key.to_owned(),
    version: version.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    config: serde_json::to_value(item.to_owned()).map_err(|e| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to serialize config: {e}"),
    })?,
  };
  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    diesel::insert_into(dsl::vm_configs)
      .values(&dbmodel)
      .execute(&mut conn)
      .map_err(db_error("vm config"))?;
    Ok::<_, HttpError>(dbmodel)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = VmConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: item.name.clone(),
    version: dbmodel.version,
    vm_key: dbmodel.vm_key,
    disk: item.disk.clone(),
    host_config: item.host_config.clone(),
    hostname: item.hostname.clone(),
    user: item.user.clone(),
    labels: item.labels.clone(),
    mac_address: item.mac_address.clone(),
    password: item.password.clone(),
    ssh_key: item.ssh_key.clone(),
  };

  Ok(config)
}

/// ## Find vm config by key
///
/// Find a vm config item in database for given key
///
/// ## Arguments
///
/// - [key](uuid::Uuid) - Vm config key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmConfig) - The found vm config
///   - [Err](HttpResponseError) - Error during the operation
///
/// ## Examples
///
/// ```rust,norun
/// let config = find_by_key(uuid::Uuid::new_v4(), &pool).await;
/// ```
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> Result<VmConfig, HttpError> {
  use crate::schema::vm_configs::dsl;

  let key = *key;
  let pool = pool.clone();

  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let config = dsl::vm_configs
      .filter(dsl::key.eq(key))
      .get_result::<VmConfigDbModel>(&mut conn)
      .map_err(db_error("vm config"))?;
    Ok::<_, HttpError>(config)
  })
  .await
  .map_err(db_blocking_error)?;

  let config = serde_json::from_value::<VmConfigPartial>(dbmodel.config)
    .map_err(|e| HttpError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Failed to deserialize config: {e}"),
    })?;

  Ok(VmConfig {
    key: dbmodel.key,
    created_at: dbmodel.created_at,
    name: config.name,
    version: dbmodel.version,
    vm_key: dbmodel.vm_key,
    hostname: config.hostname,
    user: config.user,
    labels: config.labels,
    mac_address: config.mac_address,
    disk: config.disk,
    host_config: config.host_config,
    password: config.password,
    ssh_key: config.ssh_key,
  })
}

/// ## Delete vm config by vm key
///
/// Delete all vm config items in database for given vm key
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
/// let res = delete_by_vm_key(String::from("test"), &pool).await;
/// ```
///
pub async fn delete_by_vm_key(
  key: &str,
  pool: &Pool,
) -> Result<GenericDelete, HttpError> {
  use crate::schema::vm_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let res = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let res = diesel::delete(dsl::vm_configs)
      .filter(dsl::vm_key.eq(key))
      .execute(&mut conn)
      .map_err(db_error("vm config"))?;
    Ok::<_, HttpError>(res)
  })
  .await
  .map_err(db_blocking_error)?;

  Ok(GenericDelete { count: res })
}

pub async fn list_by_vm(
  key: &str,
  pool: &Pool,
) -> Result<Vec<VmConfig>, HttpError> {
  use crate::schema::vm_configs::dsl;

  let key = key.to_owned();
  let pool = pool.clone();

  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = dsl::vm_configs
      .filter(dsl::vm_key.eq(key))
      .get_results::<VmConfigDbModel>(&mut conn)
      .map_err(db_error("vm config"))?;
    Ok::<_, HttpError>(configs)
  })
  .await
  .map_err(db_blocking_error)?;

  let configs = dbmodels
    .into_iter()
    .map(|dbmodel| {
      let config = serde_json::from_value::<VmConfigPartial>(dbmodel.config)
        .map_err(|e| HttpError {
          status: StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Failed to deserialize config: {e}"),
        })?;

      Ok(VmConfig {
        key: dbmodel.key,
        created_at: dbmodel.created_at,
        name: config.name,
        version: dbmodel.version,
        vm_key: dbmodel.vm_key,
        hostname: config.hostname,
        user: config.user,
        labels: config.labels,
        mac_address: config.mac_address,
        disk: config.disk,
        host_config: config.host_config,
        ssh_key: config.ssh_key,
        password: config.password,
      })
    })
    .collect::<Result<Vec<VmConfig>, HttpError>>()?;

  Ok(configs)
}
