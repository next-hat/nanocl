#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoReplicaTask {
  /// Stable persisted Cargo replica identity.
  pub key: uuid::Uuid,
  /// Stable Cargo-wide ordinal for the replica.
  pub ordinal: i32,
}

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
  /// Endpoint to connect to the node
  pub endpoint: String,
  /// Version of the node
  pub version: String,
  /// User defined metadata
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StartNodeCargoParams {
  /// Canonical cargo key in `{namespace}.{name}` format
  pub cargo_key: String,
  /// Exact durable replicas assigned to this node, ordered by ordinal.
  pub replicas: Vec<CargoReplicaTask>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct DeleteNodeCargoParams {
  /// Canonical cargo key in `{namespace}.{name}` format
  pub cargo_key: String,
}

/// A coordinator-accepted start sent only to operation-aware node routes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StartNodeCargoOperationParams {
  pub cargo_key: String,
  pub operation_key: uuid::Uuid,
  pub incarnation_key: uuid::Uuid,
  pub spec_key: uuid::Uuid,
  /// Exact durable replicas assigned to the receiving node.
  pub replicas: Vec<CargoReplicaTask>,
}

/// An accepted deletion of this incarnation's runtime on the receiving node.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct DeleteNodeCargoOperationParams {
  pub cargo_key: String,
  pub operation_key: uuid::Uuid,
  pub incarnation_key: uuid::Uuid,
  pub spec_key: uuid::Uuid,
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::*;

  #[test]
  fn cargo_node_operation_envelopes_require_every_identity() {
    let value = serde_json::json!({
      "CargoKey": "global.api",
      "OperationKey": "11111111-1111-4111-8111-111111111111",
      "IncarnationKey": "22222222-2222-4222-8222-222222222222",
      "SpecKey": "33333333-3333-4333-8333-333333333333",
      "Replicas": [{
        "Key": "44444444-4444-4444-8444-444444444444",
        "Ordinal": 2
      }],
    });
    let start =
      serde_json::from_value::<StartNodeCargoOperationParams>(value.clone())
        .unwrap();
    assert_eq!(serde_json::to_value(start).unwrap(), value);
    assert!(
      serde_json::from_value::<DeleteNodeCargoOperationParams>(value.clone())
        .is_ok()
    );
    for field in ["CargoKey", "OperationKey", "IncarnationKey", "SpecKey"] {
      let mut missing = value.clone();
      missing.as_object_mut().unwrap().remove(field);
      assert!(
        serde_json::from_value::<StartNodeCargoOperationParams>(
          missing.clone()
        )
        .is_err(),
        "{field}"
      );
      assert!(
        serde_json::from_value::<DeleteNodeCargoOperationParams>(missing)
          .is_err(),
        "{field}"
      );
    }
    let mut missing_replicas = value;
    missing_replicas.as_object_mut().unwrap().remove("Replicas");
    assert!(
      serde_json::from_value::<StartNodeCargoOperationParams>(missing_replicas)
        .is_err()
    );
  }

  #[test]
  fn cargo_replica_task_uses_pascal_case_wire_fields() {
    let task = CargoReplicaTask {
      key: "3e8f6eef-f03a-446d-9e7a-635728c3f565".parse().unwrap(),
      ordinal: 7,
    };

    assert_eq!(
      serde_json::to_value(task).unwrap(),
      serde_json::json!({
        "Key": "3e8f6eef-f03a-446d-9e7a-635728c3f565",
        "Ordinal": 7,
      })
    );
  }

  #[test]
  fn start_node_cargo_params_round_trip_exact_replica_vector() {
    let params = StartNodeCargoParams {
      cargo_key: "global.api".to_owned(),
      replicas: vec![
        CargoReplicaTask {
          key: "3e8f6eef-f03a-446d-9e7a-635728c3f565".parse().unwrap(),
          ordinal: 0,
        },
        CargoReplicaTask {
          key: "25a8057e-7147-4d26-9de5-652cafbf119f".parse().unwrap(),
          ordinal: 4,
        },
      ],
    };

    let value = serde_json::to_value(&params).unwrap();
    assert!(value["Replicas"].is_array());
    assert_eq!(value["Replicas"][1]["Ordinal"], 4);
    assert_eq!(
      serde_json::from_value::<StartNodeCargoParams>(value).unwrap(),
      params
    );
  }

  #[test]
  fn start_node_cargo_params_rejects_the_removed_count_shape() {
    let old_count_payload = serde_json::json!({
      "CargoKey": "global.api",
      "Replicas": 2,
    });

    assert!(
      serde_json::from_value::<StartNodeCargoParams>(old_count_payload)
        .is_err()
    );
  }
}
