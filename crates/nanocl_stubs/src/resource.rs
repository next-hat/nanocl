#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::system::{EventActor, ToEvent, EventAction, Event, EventKind};

/// Payload used to create a new resource
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

/// Payload used to update a resource
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ResourceUpdate {
  /// The spec of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The metadata of the resource as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// Convert a ResourcePartial into a Resource
impl From<ResourcePartial> for ResourceUpdate {
  fn from(resource: ResourcePartial) -> Self {
    Self {
      data: resource.data,
      metadata: resource.metadata,
    }
  }
}

/// The spec of a resource once created in the system
#[derive(Debug, Clone)]
#[cfg_attr(feature = "test", derive(Default))]
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

/// Resource is a specification with a name and a kind
/// It is used to define [proxy rules](ProxyRule) and other kind of spec
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Resource {
  /// The kind of the resource
  pub kind: String,
  /// The creation date of the resource
  pub created_at: chrono::NaiveDateTime,
  /// Specification of the ressource
  pub spec: ResourceSpec,
}

/// Convert a Resource into an EventActor
impl From<Resource> for EventActor {
  fn from(resource: Resource) -> Self {
    Self {
      key: Some(resource.spec.resource_key),
      attributes: Some(serde_json::json!({
        "Kind": resource.kind,
        "Version": resource.spec.version,
        "Metadata": resource.spec.metadata,
        "Spec": resource.spec.data,
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

/// Convert a Resource into a ResourcePartial
impl From<Resource> for ResourcePartial {
  fn from(resource: Resource) -> Self {
    Self {
      name: resource.spec.resource_key,
      kind: resource.kind,
      data: resource.spec.data,
      metadata: resource.spec.metadata,
    }
  }
}
