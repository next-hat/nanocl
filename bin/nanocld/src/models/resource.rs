use serde::{Serialize, Deserialize};

use nanocl_stubs::resource::Resource;

use crate::schema::resources;

use crate::models::resource_spec::ResourceSpecDb;

use super::WithSpec;

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
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The kind of the resource
  pub kind: String,
  /// The spec key reference
  pub spec_key: uuid::Uuid,
}

impl WithSpec for ResourceDb {
  type Type = Resource;
  type Relation = ResourceSpecDb;

  fn with_spec(self, r: &Self::Relation) -> Self::Type {
    Self::Type {
      created_at: self.created_at,
      kind: self.kind,
      spec: r.clone().into(),
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
  pub key: Option<String>,
  /// The spec key reference
  pub spec_key: Option<uuid::Uuid>,
}
