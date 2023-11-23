use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::resource::ResourceSpec;

use crate::utils;
use crate::models::{Pool, ResourceSpecDb};

/// ## Create
///
/// Create a resource config in database
///
/// ## Arguments
///
/// * [item](ResourceSpecDb) - Resource config item
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [ResourceSpecDb](ResourceSpecDb)
///
pub(crate) async fn create(
  item: &ResourceSpecDb,
  pool: &Pool,
) -> IoResult<ResourceSpecDb> {
  let item = item.clone();
  super::generic::insert_with_res(item, pool).await
}

/// ## Delete by resource key
///
/// Delete all resource config by a resource key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_resource_key(
  key: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::resource_specs;
  let key = key.to_owned();
  super::generic::delete::<resource_specs::table, _>(
    resource_specs::dsl::resource_key.eq(key),
    pool,
  )
  .await
}

/// ## List by resource key
///
/// List all resource config by resource key
///
/// ## Arguments
///
/// * [key](str) - Resource key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [ResourceSpec](ResourceSpec)
///
pub(crate) async fn list_by_resource_key(
  key: &str,
  pool: &Pool,
) -> IoResult<Vec<ResourceSpec>> {
  use crate::schema::resource_specs;
  let key = key.to_owned();
  let pool = pool.clone();
  let models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items = resource_specs::dsl::resource_specs
      .order(resource_specs::dsl::created_at.desc())
      .filter(resource_specs::dsl::resource_key.eq(key))
      .load::<ResourceSpecDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceSpec"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  let models = models
    .into_iter()
    .map(ResourceSpec::from)
    .collect::<Vec<_>>();
  Ok(models)
}

/// ## Find by key
///
/// Find resource config by key
///
/// ## Arguments
///
/// * [key](uuid::Uuid) - Resource config key
/// * [pool](Pool) - Database connection pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [ResourceSpec](ResourceSpec)
///
pub(crate) async fn find_by_key(
  key: &uuid::Uuid,
  pool: &Pool,
) -> IoResult<ResourceSpec> {
  use crate::schema::resource_specs;
  let key = *key;
  let pool = pool.clone();
  let model = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = resource_specs::dsl::resource_specs
      .filter(resource_specs::dsl::key.eq(key))
      .first::<ResourceSpecDb>(&mut conn)
      .map_err(|err| err.map_err_context(|| "ResourceSpec"))?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(ResourceSpec::from(model))
}
