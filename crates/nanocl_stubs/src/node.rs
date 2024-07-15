#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Node {
  /// The name of the node
  pub name: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The ip address of the node
  #[cfg_attr(feature = "utoipa", schema(value_type = String))]
  pub ip_address: ipnet::IpNet,
  /// Endpoint to connect to the node
  pub endpoint: String,
  /// Version of the node
  pub version: String,
  /// User defined metadata
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
}
