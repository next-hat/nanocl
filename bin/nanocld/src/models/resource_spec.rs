use nanocl_stubs::resource::ResourceSpec;

use crate::schema::resource_specs;

use super::resource::ResourceDb;

/// ## ResourceSpecDb
///
/// This structure represent the resource spec in the database.
/// A resource spec represent the specification of a resource.
/// It is stored as a json object in the database.
/// We use the `resource_key` to link to the resource.
///
#[derive(Clone, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = resource_specs)]
#[diesel(belongs_to(ResourceDb, foreign_key = resource_key))]
pub struct ResourceSpecDb {
  /// The key of the resource spec
  pub key: uuid::Uuid,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The resource key reference
  pub resource_key: String,
  /// The version of the resource spec
  pub version: String,
  /// The data of the spec
  pub data: serde_json::Value,
  /// The metadata (user defined)
  pub metadata: Option<serde_json::Value>,
}

/// Helper to convert a `ResourceSpecDb` to a `ResourceSpec`
impl From<ResourceSpecDb> for ResourceSpec {
  fn from(item: ResourceSpecDb) -> Self {
    ResourceSpec {
      key: item.key,
      version: item.version,
      created_at: item.created_at,
      resource_key: item.resource_key,
      data: item.data,
      metadata: item.metadata,
    }
  }
}
