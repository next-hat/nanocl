//! Repository to manage secrets in database
//! We can create delete list or inspect a secret
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error;
use nanocl_utils::io_error::FromIo;

use nanocl_stubs::{generic, secret};

use crate::{utils, schema, models};

/// ## Create
///
/// Create a secret in database
///
/// ## Arguments
///
/// - [item](secret::SecretPartial) - Secret to create
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::SecretDbModel) - Secret created
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn create(
  item: &secret::SecretPartial,
  pool: &models::Pool,
) -> io_error::IoResult<models::SecretDbModel> {
  let item: models::SecretDbModel = item.clone().into();

  utils::repository::generic_insert_with_res(pool, item).await
}

/// ## List
///
/// List all secrets in database
///
/// ## Arguments
///
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<models::SecretDbModel>) - List of secrets
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn list(
  pool: &models::Pool,
) -> io_error::IoResult<Vec<models::SecretDbModel>> {
  use crate::schema::secrets::dsl;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let sql = dsl::secrets.into_boxed();
    // if let Some(key) = &query.key {
    //   sql = sql.filter(dsl::key.ilike(format!("%{key}%")));
    // }
    // if let Some(limit) = query.limit {
    //   sql = sql.limit(limit);
    // }
    // if let Some(offset) = query.offset {
    //   sql = sql.offset(offset);
    // }
    let items = sql
      .load(&mut conn)
      .map_err(|err| err.map_err_context(|| "Secret"))?;
    Ok::<_, io_error::IoError>(items)
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
/// - [key](str) - Key of the secret to delete
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](generic::GenericDelete) - Number of deleted secrets
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn delete_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<generic::GenericDelete> {
  let key = key.to_owned();

  utils::repository::generic_delete_by_id::<schema::secrets::table, _>(
    pool, key,
  )
  .await
}

/// ## Find by key
///
/// Find a secret by key in database
///
/// ## Arguments
///
/// - [key](str) - Name of the secret to find
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](models::SecretDbModel) - Secret found
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn find_by_key(
  key: &str,
  pool: &models::Pool,
) -> io_error::IoResult<models::SecretDbModel> {
  let key = key.to_owned();

  utils::repository::generic_find_by_id::<schema::secrets::table, _, _>(
    pool, key,
  )
  .await
}

/// ## Update by key
///
/// Update a secret item in database for given key
///
/// ## Arguments
///
/// - [key](str) - Secret key
/// - [item](secret::SecretUpdate) - New secret data
/// - [pool](models::Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The secret updated
///   - [Err](io_error::IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &secret::SecretUpdate,
  pool: &models::Pool,
) -> io_error::IoResult<models::SecretDbModel> {
  let key = key.to_owned();
  let item = item.clone();
  let mut secret = find_by_key(&key, pool).await?;
  let new_item = models::SecretUpdateDbModel {
    data: Some(item.data.clone()),
    metadata: item.metadata.clone(),
  };

  utils::repository::generic_update_by_id::<
    schema::secrets::table,
    models::SecretUpdateDbModel,
    _,
  >(pool, key, new_item)
  .await?;

  secret.data = item.data;
  secret.metadata = item.metadata;

  Ok(secret)
}

// / ## Exist by key
// /
// / Check if a secret exist by key in database
// /
// / ## Arguments
// /
// / - [key](str) - Name of the secret to check
// / - [pool](models::Pool) - Database connection pool
// /
// / ## Returns
// /
// / - [Result](Result) - The result of the operation
// /   - [Ok](bool) - Existence of the secret
// /   - [Err](io_error::IoError) - Error during the operation
// /
// pub async fn exist_by_key(key: &str, pool: &models::Pool) -> io_error::IoResult<bool> {
//   use crate::schema::secrets::dsl;
//   let key = key.to_owned();
//   let pool = pool.clone();
//   let exist = web::block(move || {
//     let mut conn = utils::store::get_pool_conn(&pool)?;
//     let exist = Arc::new(dsl::secrets)
//       .filter(dsl::key.eq(key))
//       .get_result::<models::SecretDbModel>(&mut conn)
//       .optional()
//       .map_err(|err| err.map_err_context(|| "Secret"))?;
//     Ok::<_, io_error::IoError>(exist)
//   })
//   .await?;
//   Ok(exist.is_some())
// }
