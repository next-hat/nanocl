use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoError, IoResult, FromIo};

use crate::utils;
use crate::models::{Pool, ResourceKindVersionDb};

/// Get a resource kind for his given version in database
pub(crate) async fn get_version(
  name: &str,
  version: &str,
  pool: &Pool,
) -> IoResult<ResourceKindVersionDb> {
  use crate::schema::resource_kind_versions::dsl;
  let pool = Arc::clone(pool);
  let name = name.to_owned();
  let version = version.to_owned();
  let item = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let item = dsl::resource_kind_versions
      .filter(dsl::resource_kind_name.eq(&name))
      .filter(dsl::version.eq(&version))
      .get_result(&mut conn)
      .map_err(|err| {
        err.map_err_context(|| format!("Resource {name} {version}"))
      })?;
    Ok::<_, IoError>(item)
  })
  .await?;
  Ok(item)
}
