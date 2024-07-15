#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
  cargo::CargoInspect,
  system::{EventActor, EventActorKind},
};

/// Namespace is a identifier for a set of cargoes
/// It is used to group cargoes together
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Namespace {
  /// The name as primary key of the namespace
  pub name: String,
  /// When the namespace was created
  pub created_at: chrono::NaiveDateTime,
  /// User defined metadata
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// A Namespace partial is a payload used to create a new namespace
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct NamespacePartial {
  /// Name of the namespace
  pub name: String,
  /// User defined metadata
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// A Namespace Summary is a summary of a namespace
/// It is used to list all the namespaces
/// It contains the number of cargoes and instances existing in the namespace
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NamespaceSummary {
  /// Name of the namespace
  pub name: String,
  /// Number of cargoes
  pub cargoes: usize,
  /// Number of instances
  pub instances: usize,
  /// When the namespace was created
  pub created_at: chrono::NaiveDateTime,
}

/// A Namespace Inspect is a detailed view of a namespace
/// It is used to inspect a namespace
/// It contains all the information about the namespace
/// It also contains the list of cargoes
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NamespaceInspect {
  /// Name of the namespace
  pub name: String,
  /// Number of cargoes
  pub cargoes: Vec<CargoInspect>,
}

/// Convert a Namespace into an EventActor
impl From<Namespace> for EventActor {
  fn from(namespace: Namespace) -> Self {
    Self {
      key: Some(namespace.name.clone()),
      kind: EventActorKind::Namespace,
      attributes: Some(serde_json::json!({
        "Name": namespace.name,
      })),
    }
  }
}
