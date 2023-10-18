use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::secret::{SecretPartial, SecretUpdate};

use crate::utils;
use crate::models::{Pool, SecretDbModel, SecretUpdateDbModel};

/// ## Create
///
/// Create a secret in database
///
/// ## Arguments
///
/// * [item](SecretPartial) - Secret to create
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](SecretDbModel) - Secret created
///   * [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &SecretPartial,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  let item: SecretDbModel = item.clone().into();
  super::generic::generic_insert_with_res(pool, item).await
}

/// ## List
///
/// List all secrets in database
///
/// ## Arguments
///
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<SecretDbModel>) - List of secrets
///   * [Err](IoError) - Error during the operation
///
pub async fn list(pool: &Pool) -> IoResult<Vec<SecretDbModel>> {
  use crate::schema::secrets::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let sql = dsl::secrets.into_boxed();
    let items = sql
      .load(&mut conn)
      .map_err(|err| err.map_err_context(|| "Secret"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  Ok(items)
}

/// ## Delete by key
///
/// Delete a secret by key in database
///
/// ## Arguments
///
/// * [key](str) - Key of the secret to delete
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](GenericDelete) - Number of deleted secrets
///   * [Err](IoError) - Error during the operation
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::secrets;
  let key = key.to_owned();
  super::generic::generic_delete_by_id::<secrets::table, _>(pool, key).await
}

/// ## Find by key
///
/// Find a secret by key in database
///
/// ## Arguments
///
/// * [key](str) - Name of the secret to find
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](SecretDbModel) - Secret found
///   * [Err](IoError) - Error during the operation
///
pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<SecretDbModel> {
  use crate::schema::secrets;
  let key = key.to_owned();
  super::generic::generic_find_by_id::<secrets::table, _, _>(pool, key).await
}

/// ## Update by key
///
/// Update a secret item in database for given key
///
/// ## Arguments
///
/// * [key](str) - Secret key
/// * [item](SecretUpdate) - New secret data
/// * [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](SecretDbModel) - The secret updated
///   * [Err](IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &SecretUpdate,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  use crate::schema::secrets;
  let key = key.to_owned();
  let item = item.clone();
  let mut secret = find_by_key(&key, pool).await?;
  let new_item = SecretUpdateDbModel {
    data: Some(item.data.clone()),
    metadata: item.metadata.clone(),
  };
  super::generic::generic_update_by_id::<
    secrets::table,
    SecretUpdateDbModel,
    _,
  >(pool, key, new_item)
  .await?;
  secret.data = item.data;
  secret.metadata = item.metadata;
  Ok(secret)
}
