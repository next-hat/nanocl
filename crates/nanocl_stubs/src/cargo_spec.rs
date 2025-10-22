#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use bollard_next::{
  container::Config,
  models::{HealthConfig, HostConfig},
};

#[cfg(feature = "utoipa")]
use super::generic::Any;

use crate::generic::ImagePullPolicy;

/// Enum to define the strategy to place the cargo on the nodes
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", untagged)
)]
pub enum CargoPlacementStrategy {
  Distinct,
  LeastLoaded,
  Balanced,
  UserDefined(String),
}

/// Resource requirements for the cargo
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoResourceRequirementSpec {
  /// CPU requirement in number of cores
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu: Option<usize>,
  /// Memory requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory: Option<usize>,
  /// Storage requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub storage: Option<usize>,
}

/// Way to select specific or preferred nodes to place the cargo
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoPlacementSelectorSpec {
  /// Select by labels
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<Vec<String>>,
  /// Select by regions
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// Select by groups
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub groups: Option<Vec<String>>,
  /// Select by cargoes running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cargoes: Option<Vec<String>>,
  /// Select by vms running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub vms: Option<Vec<String>>,
}

/// Structure to control the placement of the cargo
/// It can be used to define on which nodes the cargo will be deployed
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoPlacementSpec {
  /// Number of replicas to deploy on the selected nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replicas: Option<usize>,
  /// Strategy to use to place the cargo on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub strategy: Option<CargoPlacementStrategy>,
  /// Regions to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// requirements to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub require: Option<CargoPlacementSelectorSpec>,
  /// preferences to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub prefer: Option<CargoPlacementSelectorSpec>,
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
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

/// Payload used to patch a cargo
/// It will create a new [CargoSpec](CargoSpec) with the new values
/// It will keep the old values in the history
///
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
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
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
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

impl From<CargoSpecPartial> for CargoSpecUpdate {
  fn from(spec: CargoSpecPartial) -> Self {
    Self {
      name: Some(spec.name),
      init_container: spec.init_container,
      container: Some(spec.container),
      placement: spec.placement,
      resource_requirement: spec.resource_requirement,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}

/// A cargo spec is the specification of a cargo
/// It used to know the state of the cargo
/// It keep tracking of an history when you patch an existing cargo
///
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
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  //// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

impl From<CargoSpec> for CargoSpecPartial {
  fn from(spec: CargoSpec) -> Self {
    Self {
      init_container: spec.init_container,
      name: spec.name,
      placement: spec.placement,
      resource_requirement: spec.resource_requirement,
      container: spec.container,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}
