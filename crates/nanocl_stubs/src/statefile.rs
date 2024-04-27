#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::{
  job::JobPartial, secret::SecretPartial, vm_spec::VmSpecPartial,
  cargo_spec::CargoSpecPartial, resource::ResourcePartial,
};

/// Statefile argument definition to pass to the Statefile
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum StatefileArgKind {
  String,
  Number,
  Boolean,
}

impl std::str::FromStr for StatefileArgKind {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "String" => Ok(StatefileArgKind::String),
      "Number" => Ok(StatefileArgKind::Number),
      "Boolean" => Ok(StatefileArgKind::Boolean),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Invalid StatefileArgKind {s}"),
      )),
    }
  }
}

impl ToString for StatefileArgKind {
  fn to_string(&self) -> String {
    match self {
      StatefileArgKind::String => "String",
      StatefileArgKind::Number => "Number",
      StatefileArgKind::Boolean => "Boolean",
    }
    .to_owned()
  }
}

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
  pub kind: StatefileArgKind,
  /// Default value of the build arg
  pub default: Option<String>,
}

/// Statefile argument definition to pass to the Statefile
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SubStateArg {
  /// Name of the argument
  pub name: String,
  /// Value for the argument
  pub value: SubStateValue,
}

/// Statefile argument definition to pass to the Statefile
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(untagged, deny_unknown_fields, rename_all = "PascalCase")
)]
pub enum SubStateValue {
  Number(f64),
  String(String),
  Boolean(bool),
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
  pub args: Option<Vec<SubStateArg>>,
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
