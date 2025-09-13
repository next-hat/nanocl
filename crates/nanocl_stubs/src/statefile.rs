#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
  cargo_spec::CargoSpecPartial, job::JobPartial, resource::ResourcePartial,
  secret::SecretPartial, vm_spec::VmSpecPartial,
};

/// Optional metadata to enrich a Statefile with documentation
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct StatefileMetadata {
  /// Optional title for the statefile documentation
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// About the statefile whill be use under the name section
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub about: Option<String>,
  /// Long about of the statefile will be use under the description section
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub long_about: Option<String>,
  /// Optional tags associated to this statefile
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub tags: Option<Vec<String>>,
  /// Override the content of the manual page for this statefile
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub man_content: Option<String>,
}

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

/// Implement display for StatefileArgKind
/// This is used to display the kind in the statefile args
/// In a human readable format
impl std::fmt::Display for StatefileArgKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      StatefileArgKind::String => "String",
      StatefileArgKind::Number => "Number",
      StatefileArgKind::Boolean => "Boolean",
    };
    write!(f, "{data}")
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
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub default: Option<StatefileArgsValue>,
  /// Required argument
  #[cfg_attr(
    feature = "serde",
    serde(default, skip_serializing_if = "std::ops::Not::not")
  )]
  pub required: bool,
  /// Multiple values accepted for the argument
  #[cfg_attr(
    feature = "serde",
    serde(default, skip_serializing_if = "std::ops::Not::not")
  )]
  pub multiple: bool,
  /// Short description of the argument
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub description: Option<String>,
  /// Example value for the argument
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub example: Option<String>,
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
  pub value: StatefileArgsValue,
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
pub enum StatefileArgsValue {
  Number(f64),
  String(String),
  Boolean(bool),
  MultipleNumber(Vec<f64>),
  MultipleString(Vec<String>),
}

impl std::fmt::Display for StatefileArgsValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      StatefileArgsValue::Number(v) => write!(f, "{v}"),
      StatefileArgsValue::String(v) => write!(f, "{v}"),
      StatefileArgsValue::Boolean(v) => write!(f, "{v}"),
      StatefileArgsValue::MultipleNumber(v) => {
        write!(f, "[")?;
        for (i, val) in v.iter().enumerate() {
          if i > 0 {
            write!(f, ", ")?;
          }
          write!(f, "{val}")?;
        }
        write!(f, "]")
      }
      StatefileArgsValue::MultipleString(v) => {
        write!(f, "[")?;
        for (i, val) in v.iter().enumerate() {
          if i > 0 {
            write!(f, ", ")?;
          }
          write!(f, "{val}")?;
        }
        write!(f, "]")
      }
    }
  }
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
  /// Optional metadata about the statefile
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<StatefileMetadata>,
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
