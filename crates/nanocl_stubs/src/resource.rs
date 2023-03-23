#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

/// Resource partial is a payload used to create a new resource
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePartial {
  /// The name of the resource
  pub name: String,
  /// The kind of the resource
  pub kind: String,
  /// Version of the config
  pub version: String,
  /// The config of the resource
  pub config: serde_json::Value,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourcePatch {
  /// Version of the config
  pub version: String,
  /// The config of the resource
  pub config: serde_json::Value,
}

/// Resource is a configuration with a name and a kind
/// It is used to define [proxy rules](ProxyRule) and other kind of config
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Resource {
  /// The name of the resource
  pub name: String,
  /// The creation date of the resource
  pub created_at: chrono::NaiveDateTime,
  /// The update date of the resource
  pub updated_at: chrono::NaiveDateTime,
  /// Version of the resource
  pub version: String,
  /// The kind of the resource
  pub kind: String,
  /// The config of the resource
  pub config_key: uuid::Uuid,
  /// The config of the resource
  pub config: serde_json::Value,
}

impl From<Resource> for ResourcePartial {
  fn from(resource: Resource) -> Self {
    Self {
      name: resource.name,
      kind: resource.kind,
      version: resource.version,
      config: resource.config,
    }
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceConfig {
  pub key: uuid::Uuid,
  pub version: String,
  pub resource_key: String,
  pub data: serde_json::Value,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ResourceQuery {
  pub kind: Option<String>,
  pub contains: Option<String>,
}
