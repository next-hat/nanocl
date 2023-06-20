use nanocl_stubs::resource::ResourceConfig;

use crate::schema::resource_configs;
use super::resource::ResourceDbModel;

/// ## ResourceConfigDbModel
///
/// This structure represent the resource config in the database.
/// A resource config represent the configuration of a resource.
/// It is stored as a json object in the database.
/// We use the `resource_key` to link to the resource.
///
#[derive(Clone, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = resource_configs)]
#[diesel(belongs_to(ResourceDbModel, foreign_key = resource_key))]
pub struct ResourceConfigDbModel {
  /// The key of the resource config
  pub(crate) key: uuid::Uuid,
  /// The created at date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The resource key reference
  pub(crate) resource_key: String,
  /// The version of the resource config
  pub(crate) version: String,
  /// The data of the config
  pub(crate) data: serde_json::Value,
}

/// Helper to convert a `ResourceConfigDbModel` to a `ResourceConfig`
impl From<ResourceConfigDbModel> for ResourceConfig {
  fn from(item: ResourceConfigDbModel) -> Self {
    ResourceConfig {
      key: item.key,
      version: item.version,
      created_at: item.created_at,
      resource_key: item.resource_key,
      config: item.data,
    }
  }
}
