use uuid::Uuid;
use diesel::prelude::*;
use serde::{Serialize, Deserialize};

use nanocl_error::io::IoResult;

use nanocl_stubs::metric::MetricPartial;

use crate::{utils, schema::metrics};

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
      expires_at: chrono::Utc::now().naive_utc() + chrono::Duration::days(30),
      node_name: p.node_name.clone(),
      kind: p.kind.clone(),
      data: p.data.clone(),
      note: p.note.clone(),
    }
  }
}
