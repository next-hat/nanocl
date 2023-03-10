#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::cargo::CargoInspect;

/// Namespace is a identifier for a set of cargoes
/// It is used to group cargoes together
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Namespace {
  /// Name of the namespace
  pub name: String,
}

#[derive(Debug, Clone)]
/// A Namespace partial is a payload used to create a new namespace
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NamespacePartial {
  /// Name of the namespace
  pub name: String,
}

/// A Namespace Summary is a summary of a namespace
/// It is used to list all the namespaces
/// It contains the number of cargoes and instances existing in the namespace
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NamespaceSummary {
  /// Name of the namespace
  pub name: String,
  /// Number of cargoes
  pub cargoes: i64,
  /// Number of instances
  pub instances: i64,
}

/// A Namespace Inspect is a detailed view of a namespace
/// It is used to inspect a namespace
/// It contains all the information about the namespace
/// It also contains the list of cargoes
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct NamespaceInspect {
  /// Name of the namespace
  pub name: String,
  /// Number of cargoes
  pub cargoes: Vec<CargoInspect>,
  // Network of the namespace
  // pub network: NetworkInspect,
}
