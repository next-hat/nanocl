#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Metric entry
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Metric {
  pub key: uuid::Uuid,
  pub created_at: chrono::NaiveDateTime,
  pub expire_at: chrono::NaiveDateTime,
  pub node_name: String,
  pub kind: String,
  pub data: serde_json::Value,
}

/// Filter metrics query
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct MetricFilterQuery {
  pub kind: String,
}
