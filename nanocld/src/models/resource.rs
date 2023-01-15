use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};

use crate::schema::{resources, resource_configs};

#[derive(Debug, Eq, PartialEq, DbEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[DbValueStyle = "snake_case"]
pub enum ResourceKind {
  ProxyRule,
}

#[derive(
  Debug, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = resources)]
pub struct ResourceDbModel {
  pub(crate) key: String,
  pub(crate) kind: ResourceKind,
  pub(crate) config_key: uuid::Uuid,
}

/// A cargo config item is the object stored in database
#[derive(Clone, Queryable, Identifiable, Insertable, Associations)]
#[diesel(primary_key(key))]
#[diesel(table_name = resource_configs)]
#[diesel(belongs_to(ResourceDbModel, foreign_key = resource_key))]
pub struct ResourceConfigDbModel {
  pub(crate) key: uuid::Uuid,
  pub(crate) resource_key: String,
  pub(crate) config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourcePartial {
  pub(crate) name: String,
  pub(crate) kind: ResourceKind,
  pub(crate) config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resource {
  pub(crate) name: String,
  pub(crate) kind: ResourceKind,
  pub(crate) config_key: uuid::Uuid,
  pub(crate) config: serde_json::Value,
}
