#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::system::{EventActor, ToEvent, EventAction, Event, EventKind};

/// Resource partial is a payload used to create a new resource
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourcePartial {
  /// The name of the resource
  pub name: String,
  /// The kind of the resource
  pub kind: String,
  /// Version of the data
  pub version: String,
  /// The data of the resource (json object)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The metadata of the resource (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceUpdate {
  /// Version of the config
  pub version: String,
  /// The config of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The config of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

impl From<ResourcePartial> for ResourceUpdate {
  fn from(resource: ResourcePartial) -> Self {
    Self {
      version: resource.version,
      data: resource.data,
      metadata: resource.metadata,
    }
  }
}

/// Resource is a configuration with a name and a kind
/// It is used to define [proxy rules](ProxyRule) and other kind of config
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
  /// The config of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The metadata of the resource (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// Convert a Resource into an EventActor
impl From<Resource> for EventActor {
  fn from(resource: Resource) -> Self {
    Self {
      key: Some(resource.name),
      attributes: Some(serde_json::json!({
        "Kind": resource.kind,
        "Version": resource.version,
        "Metadata": resource.metadata,
        "Spec": resource.data,
      })),
    }
  }
}

/// Implement ToEvent for Resource to generate an event
impl ToEvent for Resource {
  fn to_event(&self, action: EventAction) -> Event {
    Event {
      kind: EventKind::Resource,
      action,
      actor: Some(self.clone().into()),
    }
  }
}

impl From<Resource> for ResourcePartial {
  fn from(resource: Resource) -> Self {
    Self {
      name: resource.name,
      kind: resource.kind,
      version: resource.version,
      data: resource.data,
      metadata: resource.metadata,
    }
  }
}

/// ## ResourceConfig
///
/// The config of the resource
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceSpec {
  /// Key of the resource
  pub key: uuid::Uuid,
  /// Version of the resource
  pub version: String,
  /// The creation date of the resource
  pub created_at: chrono::NaiveDateTime,
  /// Resource key associated with the data
  pub resource_key: String,
  /// The data of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The metadata of the resource (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// ResourceQuery
///
/// Query filter when listing resources
#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceQuery {
  /// The kind of resource to target
  pub kind: Option<String>,
  /// Match what contains the resource data
  pub contains: Option<String>,
  /// Test if key exist in the resource data
  pub exists: Option<String>,
  /// Match what contains the metadata of the resource
  pub meta_contains: Option<String>,
  /// Test if key exist in the metadata of the resource
  pub meta_exists: Option<String>,
}
