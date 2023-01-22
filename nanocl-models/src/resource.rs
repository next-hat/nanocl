#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[cfg(feature = "diesel")]
use diesel_derive_enum::DbEnum;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "diesel", derive(DbEnum))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "diesel", DbValueStyle = "snake_case")]
pub enum ResourceKind {
  ProxyRule,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePartial {
  pub name: String,
  pub kind: ResourceKind,
  pub config: serde_json::Value,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Resource {
  pub name: String,
  pub kind: ResourceKind,
  pub config_key: uuid::Uuid,
  pub config: serde_json::Value,
}
