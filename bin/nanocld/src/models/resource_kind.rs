use std::sync::Arc;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{utils, gen_where4string};
use crate::schema::{resource_kinds, resource_kind_versions};

use super::{Repository, Pool};

/// This structure represent the resource kind verion in the database.
/// A resource kind version represent the version of a resource kind.
/// It is stored as a json object in the database.
/// We use the `resource_kind_name` to link to the resource kind.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(table_name = resource_kind_versions)]
#[diesel(primary_key(resource_kind_name, version))]
pub struct ResourceKindVersionDb {
  /// The related resource kind reference
  pub resource_kind_name: String,
  /// When the resource kind version have been created
  pub created_at: chrono::NaiveDateTime,
  /// The version of the resource kind
  pub version: String,
  /// The JSONSchema of the resource of this kind and version
  pub schema: Option<serde_json::Value>,
  /// The service to call when creating, updating or deleting a resource of this kind and version
  pub url: Option<String>,
}

/// This structure represent the resource kind in the database.
/// A resource kind represent the kind of a resource.
/// It is stored with a version that containt the schema or and url of a service to call.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(name))]
#[diesel(table_name = resource_kinds)]
pub struct ResourceKindDb {
  /// Name of the kind
  pub name: String,
  /// When the kind have been created
  pub created_at: chrono::NaiveDateTime,
}

/// This structure is a partial representation of a resource kind.
/// It is used to create a resource kind in the database.
#[derive(Clone, Debug)]
pub struct ResourceKindPartial {
  /// The name of the resource kind
  pub name: String,
  /// The version of the resource kind
  pub version: String,
  /// The JSONSchema of the resource of this kind and version
  pub schema: Option<serde_json::Value>,
  /// The service to call when creating, updating or deleting a resource of this kind and version
  pub url: Option<String>,
}

impl From<&ResourceKindPartial> for ResourceKindVersionDb {
  fn from(p: &ResourceKindPartial) -> Self {
    ResourceKindVersionDb {
      resource_kind_name: p.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      version: p.version.clone(),
      schema: p.schema.clone(),
      url: p.url.clone(),
    }
  }
}

impl Repository for ResourceKindVersionDb {
  type Table = resource_kind_versions::table;
  type Item = ResourceKindVersionDb;
  type UpdateItem = ResourceKindVersionDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("ResourceKindVersionDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query =
      resource_kind_versions::dsl::resource_kind_versions.into_boxed();
    if let Some(value) = r#where.get("resource_kind_name") {
      gen_where4string!(
        query,
        resource_kind_versions::dsl::resource_kind_name,
        value
      );
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("ResourceKindVersionDb::find filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query =
      resource_kind_versions::dsl::resource_kind_versions.into_boxed();
    if let Some(value) = r#where.get("resource_kind_name") {
      gen_where4string!(
        query,
        resource_kind_versions::dsl::resource_kind_name,
        value
      );
    }
    if let Some(value) = r#where.get("version") {
      gen_where4string!(query, resource_kind_versions::dsl::version, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }
}

impl ResourceKindVersionDb {
  pub(crate) async fn get_version(
    name: &str,
    version: &str,
    pool: &Pool,
  ) -> IoResult<ResourceKindVersionDb> {
    let filter = GenericFilter::new()
      .r#where("resource_kind_name", GenericClause::Eq(name.to_owned()))
      .r#where("version", GenericClause::Eq(version.to_owned()));
    ResourceKindVersionDb::find_one(&filter, pool).await?
  }
}

impl From<&ResourceKindPartial> for ResourceKindDb {
  fn from(p: &ResourceKindPartial) -> Self {
    ResourceKindDb {
      name: p.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
    }
  }
}

impl Repository for ResourceKindDb {
  type Table = resource_kinds::table;
  type Item = ResourceKindDb;
  type UpdateItem = ResourceKindDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("ResourceKindDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::dsl::resource_kinds.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::dsl::name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("ResourceKindDb::find filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = resource_kinds::dsl::resource_kinds.into_boxed();
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, resource_kinds::dsl::name, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      Ok::<_, IoError>(items)
    })
  }
}
