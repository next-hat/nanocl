use nanocl_macros_getters::{repository_delete_by, repository_create};
use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::vm_config::{VmConfig, VmConfigPartial};

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

use crate::serializers::vm_config::serialize_vm_config;
use crate::utils;
use crate::models::{Pool, VmConfigDbModel};

/// ## Create
///
/// Create a vm config item in database for given `VmConfigPartial`
/// and return a `VmConfig` with the generated key
///
/// ## Arguments
///
/// - [vm_key](str) - Vm key
/// - [item](VmConfigPartial) - Vm config item
/// - [version](str) - Vm config version
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmConfig) - The created vm config
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  vm_key: &str,
  item: &VmConfigPartial,
  version: &str,
  pool: &Pool,
) -> IoResult<VmConfig> {
  use crate::schema::vm_configs::dsl;
  let pool = pool.clone();
  let mut config = serde_json::to_value(item.to_owned())
    .map_err(|err| err.map_err_context(|| "VmConfig"))?;
  if let Some(meta) = config.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = VmConfigDbModel {
    key: uuid::Uuid::new_v4(),
    vm_key: vm_key.to_owned(),
    version: version.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    config,
    metadata: item.metadata.clone(),
  };
  let dbmodel = repository_create!(dsl::vm_configs, dbmodel, pool, "VmConfig");
  // let dbmodel = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   diesel::insert_into(dsl::vm_configs)
  //     .values(&dbmodel)
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "VmConfig"))?;
  //   Ok::<_, IoError>(dbmodel)
  // })
  // .await?;
  let config = serialize_vm_config(dbmodel, item);
  Ok(config)
}

/// ## Find by key
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
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_key(key: &uuid::Uuid, pool: &Pool) -> IoResult<VmConfig> {
  use crate::schema::vm_configs::dsl;
  let key = *key;
  let pool = pool.clone();
  let dbmodel = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let config = dsl::vm_configs
      .filter(dsl::key.eq(key))
      .get_result::<VmConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmConfig"))?;
    Ok::<_, IoError>(config)
  })
  .await?;
  let config =
    serde_json::from_value::<VmConfigPartial>(dbmodel.config.clone())
      .map_err(|err| err.map_err_context(|| "VmConfigPartial"))?;
  Ok(serialize_vm_config(dbmodel, &config))
}

/// ## Delete by vm key
///
/// Delete all vm config items in database for given vm key
///
/// ## Arguments
///
/// - [key](str) - Vm key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The number of deleted items
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_vm_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::vm_configs::dsl;
  let res =
    repository_delete_by!(dsl::vm_configs, dsl::vm_key, key, pool, "VmConfig");
  // let key = key.to_owned();
  // let pool = pool.clone();
  // let res = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let res = diesel::delete(dsl::vm_configs)
  //     .filter(dsl::vm_key.eq(key))
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "VmConfig"))?;
  //   Ok::<_, IoError>(res)
  // })
  // .await?;
  Ok(GenericDelete { count: res })
}

/// ## List by vm key
///
/// List all vm config items in database for given vm key
///
/// ## Arguments
///
/// - [key](str) - Vm key
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<VmConfig>) - The list of vm configs
///   - [Err](IoError) - Error during the operation
///
pub async fn list_by_vm_key(key: &str, pool: &Pool) -> IoResult<Vec<VmConfig>> {
  use crate::schema::vm_configs::dsl;
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = dsl::vm_configs
      .filter(dsl::vm_key.eq(key))
      .get_results::<VmConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "VmConfig"))?;
    Ok::<_, IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel| {
      let config =
        serde_json::from_value::<VmConfigPartial>(dbmodel.config.clone())
          .map_err(|err| err.map_err_context(|| "VmConfigPartial"))?;
      Ok(serialize_vm_config(dbmodel, &config))
    })
    .collect::<Result<Vec<VmConfig>, IoError>>()?;
  Ok(configs)
}
