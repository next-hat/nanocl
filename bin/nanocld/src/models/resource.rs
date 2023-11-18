use diesel::prelude::*;

use serde::{Deserialize, Serialize};

use nanocl_stubs::resource;

use crate::schema::resources;

/// ## ResourceDbModel
///
/// This structure represent a resource in the database.
/// A resource is a representation of a configuration for internal nanocl services (controllers).
/// Custom `kind` can be added to the system.
/// We use the `config_key` to link to the resource config.
/// The `key` is used to identify the resource.
/// The `kind` is used to know which controller to use.
///
#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = resources)]
pub struct ResourceDbModel {
  /// The key of the resource
  pub(crate) key: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The kind of the resource
  pub(crate) kind: String,
  /// The spec key reference
  pub(crate) spec_key: uuid::Uuid,
}

impl ResourceDbModel {
  pub fn into_resource(
    self,
    config: super::ResourceConfigDbModel,
  ) -> resource::Resource {
    resource::Resource {
      name: self.key,
      created_at: self.created_at,
      updated_at: config.created_at,
      kind: self.kind,
      version: config.version,
      config_key: config.key,
      spec: config.data,
      metadata: config.metadata,
    }
  }
}

/// ## ResourceUpdateModel
///
/// This structure represent the update of a resource in the database.
///
#[derive(AsChangeset)]
#[diesel(table_name = resources)]
pub struct ResourceUpdateModel {
  /// The key of the resource
  pub(crate) key: Option<String>,
  /// The config key reference
  pub(crate) spec_key: Option<uuid::Uuid>,
}

/// ## ResourceRevertPath
///
/// This structure is used to parse the path of the url of the revert endpoint.
///
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceRevertPath {
  /// The version
  pub version: String,
  /// The name
  pub name: String,
  /// The history id
  pub id: uuid::Uuid,
}
