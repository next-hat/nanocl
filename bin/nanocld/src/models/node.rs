use diesel::prelude::*;
use serde::{Serialize, Deserialize};

use crate::schema::nodes;

/// This structure represent a node in the database.
/// A node is a machine that is connected to nanocl network.
#[derive(
  Debug, Clone, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = nodes)]
#[serde(rename_all = "PascalCase")]
pub struct NodeDb {
  /// The name of the node
  pub name: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The ip address of the node
  pub ip_address: ipnet::IpNet,
  /// Endpoint to connect to the node
  pub endpoint: String,
  /// Version of the node
  pub version: String,
  /// User defined metadata
  pub metadata: Option<serde_json::Value>,
}
