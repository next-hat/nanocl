use serde::{Serialize, Deserialize};

use nanocl_stubs::resource::Resource;

use crate::schema::resources;

use crate::models::resource_spec::ResourceSpecDb;

/// ## ResourceDb
///
/// This structure represent a resource in the database.
/// A resource is a representation of a specification for internal nanocl services (controllers).
/// Custom `kind` can be added to the system.
/// We use the `spec_key` to link to the resource spec.
/// The `key` is used to identify the resource.
/// The `kind` is used to know which controller to use.
///
#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = resources)]
pub struct ResourceDb {
  /// The key of the resource
  pub(crate) key: String,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The kind of the resource
  pub(crate) kind: String,
  /// The spec key reference
  pub(crate) spec_key: uuid::Uuid,
}

impl ResourceDb {
  pub fn into_resource(self, spec: ResourceSpecDb) -> Resource {
    Resource {
      name: self.key,
      created_at: self.created_at,
      updated_at: spec.created_at,
      kind: self.kind,
      version: spec.version,
      spec_key: spec.key,
      data: spec.data,
      metadata: spec.metadata,
    }
  }
}

/// ## ResourceUpdateDb
///
/// This structure represent the update of a resource in the database.
///
#[derive(AsChangeset)]
#[diesel(table_name = resources)]
pub struct ResourceUpdateDb {
  /// The key of the resource
  pub(crate) key: Option<String>,
  /// The spec key reference
  pub(crate) spec_key: Option<uuid::Uuid>,
}
