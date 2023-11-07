use serde::{Serialize, Deserialize};
use bollard_next::service::ContainerConfig;

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Job {
  /// Name of the job
  pub name: String,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Secrets to load as environment variables
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Containers to run
  pub containers: Vec<ContainerConfig>,
}

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct JobPartial {
  /// Name of the job
  pub name: String,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Secrets to load as environment variables
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// List of container to run
  pub containers: Vec<ContainerConfig>,
}

/// ## JobUpdate
///
/// Payload used to update a job
///
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct JobUpdate {
  /// New name of the job
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// New secrets to load as environment variables
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// New metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// New containers to run
  pub containers: Option<Vec<ContainerConfig>>,
}
