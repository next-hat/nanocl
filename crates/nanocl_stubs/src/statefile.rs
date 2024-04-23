#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::{
  job::JobPartial, secret::SecretPartial, vm_spec::VmSpecPartial,
  cargo_spec::CargoSpecPartial, resource::ResourcePartial,
};

/// Statefile argument definition to pass to the Statefile
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct StatefileArg {
  /// Name of the build arg
  pub name: String,
  /// Kind of the build arg
  pub kind: String,
  /// Default value of the build arg
  pub default: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SubStateDef {
  pub path: String,
  pub args: Option<Vec<StatefileArg>>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(untagged, deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum SubState {
  Path(String),
  Definition(SubStateDef),
}

/// Structure that represent a Statefile
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct Statefile {
  /// Api version to use or remote url
  pub api_version: String,
  /// Arguments to pass to the Statefile
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub args: Option<Vec<StatefileArg>>,
  /// Include sub states that will be applied before the current state
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub sub_states: Option<Vec<SubState>>,
  /// Set the group of defined objects default to `{name_of_directory}.{name_of_file}`
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub group: Option<String>,
  /// Namespace where the cargoes and virtual machines are deployed
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub namespace: Option<String>,
  /// List of secrets to create
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<SecretPartial>>,
  /// List of resources to create
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resources: Option<Vec<ResourcePartial>>,
  /// List of cargoes to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cargoes: Option<Vec<CargoSpecPartial>>,
  /// List of virtual machines to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub virtual_machines: Option<Vec<VmSpecPartial>>,
  /// List of jobs to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub jobs: Option<Vec<JobPartial>>,
}
