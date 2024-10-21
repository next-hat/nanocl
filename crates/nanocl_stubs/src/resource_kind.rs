#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::generic::Any;

/// Specification of a resource kind.
/// Depending on the spec it will validate a JSONSchema or call a service.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceKindSpec {
  /// The JSONSchema of the resource of this kind and version
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub schema: Option<serde_json::Value>,
  /// The service to call when creating, updating or deleting a resource of this kind and version
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub url: Option<String>,
}

/// This structure is a partial representation of a resource kind.
/// Used to define a new kind type for plugins.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceKindPartial {
  /// The name of the resource kind
  pub name: String,
  /// The version of the resource kind
  pub version: String,
  /// Metadata (user defined) of the resource kind
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Spec of the resource kind
  pub data: ResourceKindSpec,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceKind {
  /// Name of the kind
  pub name: String,
  /// When the kind have been created
  pub version: String,
  /// When the kind have been created
  pub created_at: chrono::NaiveDateTime,
  /// When the kind have been created
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
  /// When the kind have been created
  pub data: ResourceKindSpec,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceKindVersion {
  /// Key of the version
  pub key: uuid::Uuid,
  /// When the version have been created
  pub created_at: chrono::NaiveDateTime,
  /// Kind linked to this version
  pub kind_key: String,
  /// Version
  pub version: String,
  /// Metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Spec of the kind
  pub data: ResourceKindSpec,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceKindInspect {
  /// Name of the kind
  pub name: String,
  /// When the kind have been created
  pub created_at: chrono::NaiveDateTime,
  /// List of versions available
  pub versions: Vec<ResourceKindVersion>,
}
