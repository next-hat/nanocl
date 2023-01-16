use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};

use crate::schema::resources;

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

pub struct ResourcePathPartial {
  
}
