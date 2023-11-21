use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::secret::{SecretUpdate, SecretPartial, SecretQuery};

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
/// ## Return
///
/// [IoResult](IoResult) containing a [SecretDbModel](SecretDbModel)
///
pub(crate) async fn create(
  item: &SecretPartial,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  let item: SecretDbModel = item.clone().into();
  super::generic::insert_with_res(item, pool).await
}

/// ## List
///
/// List all secrets in database
///
/// ## Arguments
///
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [SecretDbModel](SecretDbModel)
///
pub(crate) async fn list(
  query: Option<SecretQuery>,
  pool: &Pool,
) -> IoResult<Vec<SecretDbModel>> {
  use crate::schema::secrets;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let req = match query {
      Some(qs) => {
        let mut req = secrets::table.into_boxed();
        if let Some(kind) = &qs.kind {
          req = req.filter(secrets::kind.eq(kind.to_owned()));
        }
        if let Some(exists) = &qs.exists {
          req = req.filter(secrets::data.has_key(exists));
        }
        if let Some(contains) = &qs.contains {
          let contains = serde_json::from_str::<serde_json::Value>(contains)
            .map_err(|err| err.map_err_context(|| "Contains"))?;
          req = req.filter(secrets::data.contains(contains));
        }
        if let Some(meta_exists) = &qs.meta_exists {
          req = req.filter(secrets::metadata.has_key(meta_exists));
        }
        if let Some(meta_contains) = &qs.meta_contains {
          let meta_contains =
            serde_json::from_str::<serde_json::Value>(meta_contains)
              .map_err(|err| err.map_err_context(|| "Meta contains"))?;
          req = req.filter(secrets::metadata.contains(meta_contains));
        }
        req = req.order(secrets::created_at.desc());
        req.load(&mut conn)
      }
      None => secrets::table
        .order(secrets::created_at.desc())
        .load(&mut conn),
    };
    let res = req.map_err(|err| err.map_err_context(|| "Resource"))?;
    Ok::<_, IoError>(res)
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
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::secrets;
  let key = key.to_owned();
  super::generic::delete_by_id::<secrets::table, _>(key, pool).await
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
/// ## Return
///
/// [IoResult](IoResult) containing a [SecretDbModel](SecretDbModel)
///
pub(crate) async fn find_by_key(
  key: &str,
  pool: &Pool,
) -> IoResult<SecretDbModel> {
  use crate::schema::secrets;
  let key = key.to_owned();
  super::generic::find_by_id::<secrets::table, _, _>(key, pool).await
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
/// ## Return
///
/// [IoResult](IoResult) containing a [SecretDbModel](SecretDbModel)
///
pub(crate) async fn update_by_key(
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
  super::generic::update_by_id::<secrets::table, SecretUpdateDbModel, _>(
    key, new_item, pool,
  )
  .await?;
  secret.data = item.data;
  secret.metadata = item.metadata;
  Ok(secret)
}
