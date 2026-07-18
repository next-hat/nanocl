#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use bollard_next::service::Network as DockerNetwork;

/// Generate the stable database key for a node-scoped Docker network.
///
/// The key is intentionally treated as opaque. Node and network names may
/// themselves contain dots, so consumers must not split it to recover either
/// component.
pub fn gen_network_key(node_name: &str, network_name: &str) -> String {
  format!("{node_name}.{network_name}")
}

/// A Docker network snapshot stored by Nanocl for one node.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "utoipa", schema(as = nanocl::Network))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(rename = "NanoclNetwork"))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Network {
  /// Stable key in the form `{node_name}.{network_name}`.
  pub key: String,
  /// When this network was first recorded.
  pub created_at: chrono::NaiveDateTime,
  /// When the Docker inspection snapshot was last refreshed.
  pub updated_at: chrono::NaiveDateTime,
  /// Docker network name.
  pub name: String,
  /// Node whose Docker daemon owns this network.
  pub node_name: String,
  /// Complete Docker network inspection response.
  pub data: DockerNetwork,
  /// User-defined metadata retained across snapshot refreshes.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// A network inspection snapshot ready to be persisted.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NetworkPartial {
  /// Stable key in the form `{node_name}.{network_name}`.
  pub key: String,
  /// Docker network name.
  pub name: String,
  /// Node whose Docker daemon owns this network.
  pub node_name: String,
  /// Complete Docker network inspection response.
  pub data: DockerNetwork,
  /// Initial metadata. Existing metadata is preserved during an upsert.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

impl NetworkPartial {
  pub fn new(
    node_name: impl Into<String>,
    name: impl Into<String>,
    data: DockerNetwork,
  ) -> Self {
    let node_name = node_name.into();
    let name = name.into();
    Self {
      key: gen_network_key(&node_name, &name),
      name,
      node_name,
      data,
      metadata: None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn network_key_is_deterministic_and_opaque() {
    assert_eq!(
      gen_network_key("edge.eu.example", "private.api"),
      "edge.eu.example.private.api"
    );
  }

  #[test]
  fn partial_derives_its_key() {
    let partial = NetworkPartial::new(
      "node-a",
      "private-api",
      DockerNetwork {
        name: Some("private-api".to_owned()),
        ..Default::default()
      },
    );
    assert_eq!(partial.key, "node-a.private-api");
    assert_eq!(partial.data.name.as_deref(), Some("private-api"));
  }
}
