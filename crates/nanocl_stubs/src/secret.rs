#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::system::{EventActor, ToEvent, EventAction, EventKind, Event};

/// A partial secret object. This is used to create a secret.
/// A secret is a key/value pair that can be used by the user to store
/// sensitive data. It is stored as a json object in the database.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SecretPartial {
  /// The key of the secret
  pub key: String,
  /// The kind of secret
  pub kind: String,
  /// The secret cannot be updated
  pub immutable: Option<bool>,
  /// The secret data
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

/// This structure represent the secret in the database.
/// A secret is a key/value pair that can be used by the user to store
/// sensitive data. It is stored as a json object in the database.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Secret {
  /// The key of the secret
  pub key: String,
  /// The creation date
  pub created_at: chrono::NaiveDateTime,
  /// The last update date
  pub updated_at: chrono::NaiveDateTime,
  /// The kind of secret
  pub kind: String,
  /// The secret cannot be updated
  pub immutable: bool,
  /// The secret data
  pub data: serde_json::Value,
  // The metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

impl From<Secret> for SecretPartial {
  fn from(db: Secret) -> Self {
    SecretPartial {
      key: db.key,
      kind: db.kind,
      immutable: Some(db.immutable),
      data: db.data,
      metadata: db.metadata,
    }
  }
}

/// Convert a Secret into an EventActor
impl From<Secret> for EventActor {
  fn from(secret: Secret) -> Self {
    Self {
      key: Some(secret.key),
      attributes: Some(serde_json::json!({
        "Kind": secret.kind,
        "Metadata": secret.metadata,
      })),
    }
  }
}

/// Implement ToEvent for Secret to generate an event
impl ToEvent for Secret {
  fn to_event(&self, action: EventAction) -> Event {
    Event {
      kind: EventKind::Secret,
      action,
      actor: Some(self.clone().into()),
    }
  }
}

/// This structure is used to update a secret.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SecretUpdate {
  /// The data of the secret as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
  /// The metadata of the secret (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}
