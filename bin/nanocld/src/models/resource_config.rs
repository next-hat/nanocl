use crate::schema::resource_configs;
use super::resource::ResourceDbModel;

/// A cargo config item is the object stored in database
#[derive(Clone, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = resource_configs)]
#[diesel(belongs_to(ResourceDbModel, foreign_key = resource_key))]
pub struct ResourceConfigDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) resource_key: String,
  pub(crate) data: serde_json::Value,
}
