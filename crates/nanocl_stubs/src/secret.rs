#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::system::{EventActor, EventActorKind};

/// A partial secret object. This is used to create a secret.
/// A secret is a key/value pair that can be used by the user to store
/// sensitive data. It is stored as a json object in the database.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SecretPartial {
  /// The name of the secret
  pub name: String,
  /// The kind of secret
  pub kind: String,
  /// The secret cannot be updated
  pub immutable: Option<bool>,
  /// The metadata of the resource (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// The secret data
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
}

/// This structure represent the secret in the database.
/// A secret is a key/value pair that can be used by the user to store
/// sensitive data. It is stored as a json object in the database.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Secret {
  /// The name of the secret
  pub name: String,
  /// The creation date
  pub created_at: chrono::NaiveDateTime,
  /// The last update date
  pub updated_at: chrono::NaiveDateTime,
  /// The kind of secret
  pub kind: String,
  /// The secret cannot be updated
  pub immutable: bool,
  // The metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// The secret data
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
}

impl From<Secret> for SecretPartial {
  fn from(db: Secret) -> Self {
    SecretPartial {
      name: db.name,
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
      key: Some(secret.name),
      kind: EventActorKind::Secret,
      attributes: Some(serde_json::json!({
        "Kind": secret.kind,
        "Metadata": secret.metadata,
      })),
    }
  }
}

/// This structure is used to update a secret.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct SecretUpdate {
  /// The metadata of the secret (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// The data of the secret as a json object
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub data: serde_json::Value,
}

impl From<SecretPartial> for SecretUpdate {
  fn from(partial: SecretPartial) -> Self {
    SecretUpdate {
      metadata: partial.metadata,
      data: partial.data,
    }
  }
}
