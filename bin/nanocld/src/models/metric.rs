use nanocl_error::io::IoResult;
use nanocl_stubs::generic::GenericFilter;
use tokio::task::JoinHandle;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::schema::metrics;

use super::{Repository, Pool};

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
  pub expire_at: chrono::NaiveDateTime,
  /// The node where the metric come from
  pub node_name: String,
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

/// This structure is used to insert a metric in the database.
#[derive(Clone, Debug)]
pub struct MetricPartial {
  /// The kind of the metric (CPU, MEMORY, DISK, NETWORK)
  pub kind: String,
  /// The node where the metric come from
  pub node_name: String,
  /// The data of the metric
  pub data: serde_json::Value,
}

impl From<&MetricPartial> for MetricDb {
  fn from(p: &MetricPartial) -> Self {
    MetricDb {
      key: Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expire_at: chrono::Utc::now().naive_utc(),
      node_name: p.node_name.clone(),
      kind: p.kind.clone(),
      data: p.data.clone(),
    }
  }
}

impl Repository for MetricDb {
  type Table = metrics::table;
  type Item = MetricDb;
  type UpdateItem = MetricDb;

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    unimplemented!()
  }
}
