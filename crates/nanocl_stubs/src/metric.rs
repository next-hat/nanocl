#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "UPPERCASE"))]
pub enum MetricKind {
  Cpu,
  Memory,
  Network,
  Disk,
}

impl ToString for MetricKind {
  fn to_string(&self) -> String {
    match self {
      MetricKind::Cpu => "CPU",
      MetricKind::Memory => "MEMORY",
      MetricKind::Network => "NETWORK",
      MetricKind::Disk => "DISK",
    }
    .to_owned()
  }
}

/// Metric entry
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: MetricKind,
  /// The data of the metric
  pub data: serde_json::Value,
}
