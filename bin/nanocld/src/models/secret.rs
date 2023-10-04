use serde::{Serialize, Deserialize};
use nanocl_stubs::secret::{Secret, SecretPartial};

use crate::schema::secrets;

/// ## SecretDbModel
///
/// This structure represent the secret in the database.
/// A secret is a key/value pair that can be used by the user to store
/// sensitive data. It is stored as a json object in the database.
///
#[derive(
  Clone, Serialize, Deserialize, Queryable, Identifiable, Insertable,
)]
#[serde(rename_all = "PascalCase")]
#[diesel(primary_key(key))]
#[diesel(table_name = secrets)]
pub struct SecretDbModel {
  /// The key of the cargo config
  pub(crate) key: String,
  /// The creation date
  pub(crate) created_at: chrono::NaiveDateTime,
  /// The last update date
  pub(crate) updated_at: chrono::NaiveDateTime,
  /// The kind of secret
  pub(crate) kind: String,
  /// The secret cannot be updated
  pub(crate) immutable: bool,
  /// The secret data
  pub(crate) data: serde_json::Value,
  // The metadata (user defined)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) metadata: Option<serde_json::Value>,
}

impl From<SecretPartial> for SecretDbModel {
  fn from(secret: SecretPartial) -> Self {
    Self {
      key: secret.key,
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      kind: secret.kind,
      immutable: secret.immutable.unwrap_or(false),
      data: secret.data,
      metadata: secret.metadata,
    }
  }
}

impl From<SecretDbModel> for SecretPartial {
  fn from(val: SecretDbModel) -> Self {
    SecretPartial {
      key: val.key,
      kind: val.kind,
      immutable: Some(val.immutable),
      data: val.data,
      metadata: val.metadata,
    }
  }
}

impl From<SecretDbModel> for Secret {
  fn from(val: SecretDbModel) -> Self {
    Secret {
      key: val.key,
      created_at: val.created_at,
      updated_at: val.updated_at,
      kind: val.kind,
      immutable: val.immutable,
      data: val.data,
      metadata: val.metadata,
    }
  }
}

/// ## SecretUpdateDbModel
///
/// This structure is used to update a secret in the database.
///
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = secrets)]
pub struct SecretUpdateDbModel {
  /// The secret data
  pub(crate) data: Option<serde_json::Value>,
  // The metadata (user defined)
  pub(crate) metadata: Option<serde_json::Value>,
}
