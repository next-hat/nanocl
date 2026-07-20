use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use nanocl_error::io::{FromIo, IoError};
use nanocl_stubs::network::{Network, NetworkPartial};

use crate::{schema::networks, utils};

/// A node-scoped Docker network inspection stored in the database.
#[derive(
  Clone,
  Debug,
  Queryable,
  Identifiable,
  Insertable,
  Selectable,
  Serialize,
  Deserialize,
)]
#[diesel(primary_key(key))]
#[diesel(table_name = networks)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkDb {
  pub key: String,
  pub created_at: chrono::NaiveDateTime,
  pub updated_at: chrono::NaiveDateTime,
  pub name: String,
  pub node_name: String,
  pub data: serde_json::Value,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
}

impl TryFrom<&NetworkPartial> for NetworkDb {
  type Error = IoError;

  fn try_from(network: &NetworkPartial) -> Result<Self, Self::Error> {
    let expected_key =
      utils::key::gen_network_key(&network.node_name, &network.name);
    if network.key != expected_key {
      return Err(IoError::invalid_input(
        "Network key",
        &format!("expected {expected_key}, got {}", network.key),
      ));
    }
    let now = chrono::Utc::now().naive_utc();
    Ok(Self {
      key: network.key.clone(),
      created_at: now,
      updated_at: now,
      name: network.name.clone(),
      node_name: network.node_name.clone(),
      data: serde_json::to_value(&network.data)
        .map_err(|err| err.map_err_context(|| "Network"))?,
      metadata: network.metadata.clone(),
    })
  }
}

impl TryFrom<NetworkDb> for Network {
  type Error = IoError;

  fn try_from(network: NetworkDb) -> Result<Self, Self::Error> {
    Ok(Self {
      key: network.key,
      created_at: network.created_at,
      updated_at: network.updated_at,
      name: network.name,
      node_name: network.node_name,
      data: serde_json::from_value(network.data)
        .map_err(|err| err.map_err_context(|| "Network"))?,
      metadata: network.metadata,
    })
  }
}

#[cfg(test)]
mod tests {
  use bollard_next::service::Network as DockerNetwork;

  use super::*;

  #[test]
  fn rejects_a_non_deterministic_key() {
    let partial = NetworkPartial {
      key: "random-key".to_owned(),
      name: "private-api".to_owned(),
      node_name: "node-a".to_owned(),
      data: DockerNetwork::default(),
      metadata: None,
    };
    assert!(NetworkDb::try_from(&partial).is_err());
  }

  #[test]
  fn converts_typed_docker_data() {
    let partial = NetworkPartial::new(
      "node-a",
      "private-api",
      DockerNetwork {
        name: Some("private-api".to_owned()),
        driver: Some("bridge".to_owned()),
        ..Default::default()
      },
    );
    let db = NetworkDb::try_from(&partial).unwrap();
    let network = Network::try_from(db).unwrap();
    assert_eq!(network.key, "node-a.private-api");
    assert_eq!(network.data.driver.as_deref(), Some("bridge"));
  }
}
