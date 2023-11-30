use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::IoResult;

use nanocl_stubs::generic::GenericFilter;

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
  pub resource_kind_name: String,
  pub created_at: chrono::NaiveDateTime,
  pub version: String,
  pub schema: Option<serde_json::Value>,
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

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
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

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}
