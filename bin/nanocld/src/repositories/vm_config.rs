use nanocl_stubs::vm_config::VmConfigPartial;
use ntex::web;
use diesel::prelude::*;

use nanocl_stubs::{generic, vm_config};

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use crate::{utils, schema, models};

/// ## Create
///
/// Create a vm config item in database for given `vm_config::VmConfigPartial`
/// and return a `vm_config::VmConfig` with the generated key
///
/// ## Arguments
///
/// - [vm_key](str) - Vm key
/// - [item](vm_config::VmConfigPartial) - Vm config item
/// - [version](str) - Vm config version
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](vm_config::VmConfig) - The created vm config
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  vm_key: &str,
  item: &vm_config::VmConfigPartial,
  version: &str,
  pool: &models::Pool,
) -> io_error::IoResult<vm_config::VmConfig> {
  let mut config = serde_json::to_value(item.to_owned())
    .map_err(|err| err.map_err_context(|| "vm_config::VmConfig"))?;
  if let Some(meta) = config.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = models::VmConfigDbModel {
    key: uuid::Uuid::new_v4(),
    vm_key: vm_key.to_owned(),
    version: version.to_owned(),
    created_at: chrono::Utc::now().naive_utc(),
    config,
    metadata: item.metadata.clone(),
  };

  let dbmodel: models::VmConfigDbModel =
    utils::repository::generic_insert_with_res(pool, dbmodel).await?;
  Ok(dbmodel.into_vm_config(item))
}

/// ## Find by key
///
/// Find a vm config item in database for given key
///
/// ## Arguments
///
/// - [key](uuid::Uuid) - Vm config key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](vm_config::VmConfig) - The found vm config
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &uuid::Uuid,
  pool: &models::Pool,
) -> io_error::IoResult<vm_config::VmConfig> {
  let key = *key;

  let dbmodel = utils::repository::generic_find_by_id::<
    schema::vm_configs::table,
    _,
    models::VmConfigDbModel,
  >(pool, key)
  .await?;

  let config =
    serde_json::from_value::<VmConfigPartial>(dbmodel.config.clone())
      .map_err(|err| err.map_err_context(|| "VmConfigPartial"))?;
  Ok(dbmodel.into_vm_config(&config))
}
/// ## Delete by vm key
///
/// Delete all vm config items in database for given vm key
///
/// ## Arguments
///
/// - [key](str) - Vm key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [{Ok}) - The number of deleted items
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_vm_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();
  utils::repository::generic_delete_by_id::<schema::vms::table, _>(pool, key)
    .await
}

/// ## List by vm key
///
/// List all vm config items in database for given vm key
///
/// ## Arguments
///
/// - [key](str) - Vm key
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<vm_config::VmConfig>) - The list of vm configs
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list_by_vm_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<Vec<vm_config::VmConfig>> {
  use crate::schema::vm_configs::dsl;
  let key = key.to_owned();
  let pool = pool.clone();
  let dbmodels = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let configs = dsl::vm_configs
      .filter(dsl::vm_key.eq(key))
      .get_results::<models::VmConfigDbModel>(&mut conn)
      .map_err(|err| err.map_err_context(|| "vm_config::VmConfig"))?;
    Ok::<_, io_error::IoError>(configs)
  })
  .await?;
  let configs = dbmodels
    .into_iter()
    .map(|dbmodel: models::VmConfigDbModel| {
      let config = serde_json::from_value::<vm_config::VmConfigPartial>(
        dbmodel.config.clone(),
      )
      .map_err(|err| err.map_err_context(|| "vm_config::VmConfigPartial"))?;
      Ok(dbmodel.into_vm_config(&config))
    })
    .collect::<Result<Vec<vm_config::VmConfig>, io_error::IoError>>()?;
  Ok(configs)
}
