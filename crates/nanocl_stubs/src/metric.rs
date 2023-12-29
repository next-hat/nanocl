#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Metric entry
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Metric {
  /// The key of the metric in the database `UUID`
  pub key: uuid::Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expire_at: chrono::NaiveDateTime,
  /// The node where the metric come from
  pub node_name: String,
  /// The kind of the metric
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

/// Used to create a new metric
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct MetricPartial {
  /// The kind of the metric
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
}
