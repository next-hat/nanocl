use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::schema::metrics;

/// ## MetricDb
///
/// This structure represent a metric in the database.
/// A metric is a data point that can be used to monitor the system.
/// It is stored as a json object in the database.
/// We use the `node_name` to link the metric to the node.
///
#[derive(Debug, Identifiable, Queryable, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = metrics)]
pub struct MetricDb {
  /// The key of the metric in the database `UUID`
  pub key: Uuid,
  /// When the metric was created
  pub created_at: chrono::NaiveDateTime,
  /// When the metric will expire
  pub expire_at: chrono::NaiveDateTime,
  /// The node where the metric come from
  pub node_name: String,
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

/// ## MetricInsertDb
///
/// This structure is used to insert a metric in the database.
///
#[derive(Clone, Debug, Default, Insertable)]
#[diesel(table_name = metrics)]
pub struct MetricInsertDb {
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The node where the metric come from
  pub node_name: String,
  /// The data of the metric
  pub data: serde_json::Value,
}
