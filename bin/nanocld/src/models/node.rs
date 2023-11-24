use serde::{Serialize, Deserialize};

use crate::schema::nodes;

/// ## NodeDb
///
/// This structure represent a node in the database.
/// A node is a machine that is connected to nanocl network.
///
#[derive(
  Debug, Clone, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = nodes)]
#[serde(rename_all = "PascalCase")]
pub struct NodeDb {
  /// The name of the node
  pub name: String,
  /// The ip address of the node
  pub ip_address: String,
}
