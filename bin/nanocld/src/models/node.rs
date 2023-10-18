use nanocld_client::NanocldClient;
use serde::{Serialize, Deserialize};

use crate::schema::nodes;

/// ## NodeDbModel
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
pub struct NodeDbModel {
  /// The name of the node
  pub(crate) name: String,
  /// The ip address of the node
  pub(crate) ip_address: String,
}

impl NodeDbModel {
  /// ## To HTTP Client
  ///
  /// Create a nanocld client for the node from the his ip address.
  ///
  /// # Returns
  ///
  /// * [client](NanocldClient) - The client for the node
  ///
  pub fn to_http_client(&self) -> NanocldClient {
    let url =
      Box::leak(format!("http://{}:8081", self.ip_address).into_boxed_str());

    NanocldClient::connect_to(url, None)
  }
}
