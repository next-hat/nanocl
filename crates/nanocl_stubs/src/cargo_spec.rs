#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

pub use bollard_next::container::Config;
pub use bollard_next::models::HostConfig;
pub use bollard_next::models::HealthConfig;

use crate::generic::ImagePullPolicy;

/// Auto is used to automatically define that the number of replicas in the cluster
/// Number is used to manually set the number of replicas
/// Note: auto will ensure at least 1 replica exists in the cluster
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, tag = "Mode", rename_all = "PascalCase")
)]
pub enum ReplicationMode {
  /// Auto is used to automatically define that the number of replicas in the cluster
  /// This will ensure at least 1 replica exists in the cluster
  /// And automatically add more replicas in the cluster if needed for redundancy
  Auto,
  /// Unique is used to ensure that only one replica exists in the cluster
  Unique,
  /// UniqueByNode is used to ensure one replica is running on each node
  UniqueByNode,
  /// UniqueByNodeGroups is used to ensure one replica is running on each node group
  UniqueByNodeGroups { groups: Vec<String> },
  /// UniqueByNodeNames is used to ensure one replica is running on each node name
  UniqueByNodeNames { names: Vec<String> },
  /// Number is used to manually set the number of replicas in one node
  Static(ReplicationStatic),
  /// NumberByNodes is used to manually set the number of replicas in each node
  StaticByNodes(ReplicationStatic),
  /// NumberByNodeGroups is used to manually set the number of replicas in each node group
  StaticByNodeGroups { groups: Vec<String>, number: i64 },
  /// NumberByNodeNames is used to manually set the number of replicas in each node name
  StaticByNodeNames { names: Vec<String>, number: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ReplicationStatic {
  pub number: usize,
}

/// A cargo spec partial is used to create a Cargo
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoSpecPartial {
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Replication specification of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
}

/// Payload used to patch a cargo
/// It will create a new [CargoSpec](CargoSpec) with the new values
/// It will keep the old values in the history
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoSpecUpdate {
  /// New name of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// New metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// New container specification of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub container: Option<Config>,
  /// New replication specification of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
}

impl From<CargoSpecPartial> for CargoSpecUpdate {
  fn from(spec: CargoSpecPartial) -> Self {
    Self {
      name: Some(spec.name),
      init_container: spec.init_container,
      container: Some(spec.container),
      replication: spec.replication,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}

/// A cargo spec is the specification of a cargo
/// It used to know the state of the cargo
/// It keep tracking of an history when you patch an existing cargo
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSpec {
  /// Unique identifier of the cargo spec
  pub key: uuid::Uuid,
  /// The key of the cargo
  pub cargo_key: String,
  /// Version of the spec
  pub version: String,
  /// Creation date of the cargo spec
  pub created_at: chrono::NaiveDateTime,
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Replication specification of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replication: Option<ReplicationMode>,
}

impl From<CargoSpec> for CargoSpecPartial {
  fn from(spec: CargoSpec) -> Self {
    Self {
      init_container: spec.init_container,
      name: spec.name,
      replication: spec.replication,
      container: spec.container,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}
