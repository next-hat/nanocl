use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use nanocl_error::io::IoResult;

use nanocl_stubs::metric::MetricPartial;

use crate::{schema::metrics, utils};

/// This structure represent a metric in the database.
/// A metric is a data point that can be used to monitor the system.
/// It is stored as a json object in the database.
/// We use the `node_name` to link the metric to the node.
#[derive(
  Debug, Insertable, Identifiable, Queryable, Serialize, Deserialize,
)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDb {
  /// The key of the metric in the database `UUID`
  pub key: Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expires_at: chrono::NaiveDateTime,
  /// The node who saved the metric
  pub node_name: String,
  /// The kind of the metric
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
  /// Optional note about the metric
  pub note: Option<String>,
}

/// This structure is used to insert a metric in the database.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricNodePartial {
  /// The kind of the metric
  pub kind: String,
  /// The node who saved the metric
  pub node_name: String,
  /// The data of the metric
  pub data: serde_json::Value,
  /// Optional note about the metric
  pub note: Option<String>,
}

impl MetricNodePartial {
  pub fn try_new_node(node_name: &str, item: &MetricPartial) -> IoResult<Self> {
    utils::key::ensure_kind(&item.kind)?;
    Ok(MetricNodePartial {
      node_name: node_name.to_owned(),
      kind: item.kind.clone(),
      data: item.data.clone(),
      note: item.note.clone(),
    })
  }
}

impl From<&MetricNodePartial> for MetricDb {
  fn from(p: &MetricNodePartial) -> Self {
    MetricDb {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expires_at: chrono::Utc::now().naive_utc()
        + chrono::Duration::try_days(30).unwrap(),
      node_name: p.node_name.clone(),
      kind: p.kind.clone(),
      data: p.data.clone(),
      note: p.note.clone(),
    }
  }
}

/// Result of the query to find the best node to run workload
#[derive(Debug, Selectable, Queryable, QueryableByName)]
#[diesel(table_name = crate::schema::metrics)]
pub struct MetricNodeDb {
  pub node_name: String,
}

/// Weights used by the balanced node selection strategy
#[derive(Debug, Clone)]
pub struct BalanceWeights {
  pub cpu_weight: f32,
  pub mem_weight: f32,
}

/// Query parameters for capacity-based node selection.
/// This bundles optional requirements, caps, recency window and limit,
/// and optionally the weights for the balanced strategy.
#[derive(Debug, Clone)]
pub struct CapacityQuery {
  pub cpu_cores_required: Option<usize>,
  pub mem_bytes_required: Option<usize>,
  pub storage_bytes_required: Option<usize>,
  pub cpu_utilization_cap: Option<f32>,
  pub mem_utilization_cap: Option<f32>,
  pub recency_seconds: Option<i64>,
  pub limit: usize,
  /// When Some, indicates that balanced strategy should be used with these weights
  pub weights: Option<BalanceWeights>,
  /// Generic metadata constraints (require): Eq on path segments -> value
  /// Example: path ["regions"], value "eu-west-1"
  pub require_constraints_eq: Vec<(Vec<String>, String)>,
  /// Generic metadata constraints (require): Ne on path segments -> value
  pub require_constraints_ne: Vec<(Vec<String>, String)>,
}
