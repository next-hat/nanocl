//! Repository to manage secrets in database
//! We can create delete list or inspect a secret

use nanocl_macros_getters::{
  repository_create, repository_delete_by_id, repository_update_by_id,
  repository_find_by_id,
};
use ntex::web;
use diesel::prelude::*;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};

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
/// - [item](SecretPartial) - Secret to create
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](SecretDbModel) - Secret created
///   - [Err](IoError) - Error during the operation
///
pub async fn create(
  item: &SecretPartial,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  use crate::schema::secrets::dsl;
  let pool = pool.clone();
  let item: SecretDbModel = item.clone().into();
  let item = repository_create!(dsl::secrets, item, pool, "Secret");
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   diesel::insert_into(dsl::secrets)
  //     .values(&item)
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Secret"))?;
  //   Ok::<_, IoError>(item)
  // })
  // .await?;

  Ok(item)
}

/// ## List
///
/// List all secrets in database
///
/// ## Arguments
///
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<SecretDbModel>) - List of secrets
///   - [Err](IoError) - Error during the operation
///
pub async fn list(pool: &Pool) -> IoResult<Vec<SecretDbModel>> {
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
/// - [key](str) - Key of the secret to delete
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - Number of deleted secrets
///   - [Err](IoError) - Error during the operation
///
pub async fn delete_by_key(key: &str, pool: &Pool) -> IoResult<GenericDelete> {
  use crate::schema::secrets::dsl;

  let count = repository_delete_by_id!(dsl::secrets, key, pool, "Secret");

  // let key = key.to_owned();
  // let pool = pool.clone();
  // let count = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let count = diesel::delete(dsl::secrets.filter(dsl::key.eq(key)))
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Secret"))?;
  //   Ok::<_, IoError>(count)
  // })
  // .await?;
  Ok(GenericDelete { count })
}

/// ## Find by key
///
/// Find a secret by key in database
///
/// ## Arguments
///
/// - [key](str) - Name of the secret to find
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](SecretDbModel) - Secret found
///   - [Err](IoError) - Error during the operation
///
pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<SecretDbModel> {
  use crate::schema::secrets::dsl;
  let item = repository_find_by_id!(dsl::secrets, key, pool, "Secret");
  // let key = key.to_owned();
  // let pool = pool.clone();
  // let item = web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   let item = dsl::secrets
  //     .filter(dsl::key.eq(key))
  //     .get_result(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Secret"))?;
  //   Ok::<_, IoError>(item)
  // })
  // .await?;
  Ok(item)
}

/// ## Update by key
///
/// Update a secret item in database for given key
///
/// ## Arguments
///
/// - [key](str) - Secret key
/// - [item](SecretUpdate) - New secret data
/// - [pool](Pool) - Database connection pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The secret updated
///   - [Err](IoError) - Error during the operation
///
pub async fn update_by_key(
  key: &str,
  item: &SecretUpdate,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  use crate::schema::secrets::dsl;
  let key = key.to_owned();
  let item = item.clone();
  let mut secret = find_by_key(&key, &pool).await?;
  let new_item = SecretUpdateDbModel {
    data: Some(item.data.clone()),
    metadata: item.metadata.clone(),
  };

  repository_update_by_id!(dsl::secrets, key, new_item, pool, "Cargo");
  // let pool = pool.clone();
  // web::block(move || {
  //   let mut conn = utils::store::get_pool_conn(&pool)?;
  //   diesel::update(dsl::secrets.filter(dsl::key.eq(key)))
  //     .set(&new_item)
  //     .execute(&mut conn)
  //     .map_err(|err| err.map_err_context(|| "Cargo"))?;
  //   Ok::<_, IoError>(())
  // })
  // .await?;
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
// / - [pool](Pool) - Database connection pool
// /
// / ## Returns
// /
// / - [Result](Result) - The result of the operation
// /   - [Ok](bool) - Existence of the secret
// /   - [Err](IoError) - Error during the operation
// /
// pub async fn exist_by_key(key: &str, pool: &Pool) -> IoResult<bool> {
//   use crate::schema::secrets::dsl;
//   let key = key.to_owned();
//   let pool = pool.clone();
//   let exist = web::block(move || {
//     let mut conn = utils::store::get_pool_conn(&pool)?;
//     let exist = Arc::new(dsl::secrets)
//       .filter(dsl::key.eq(key))
//       .get_result::<SecretDbModel>(&mut conn)
//       .optional()
//       .map_err(|err| err.map_err_context(|| "Secret"))?;
//     Ok::<_, IoError>(exist)
//   })
//   .await?;
//   Ok(exist.is_some())
// }
