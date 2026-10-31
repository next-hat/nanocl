use diesel::prelude::*;

use crate::schema::{operations, resource_incarnations};

use super::OperationOwner;

/// Retained identity and authoritative last-operation pointer for one resource.
#[derive(Clone, Debug, PartialEq, Queryable, Identifiable, Insertable)]
#[diesel(table_name = resource_incarnations)]
#[diesel(primary_key(key))]
pub(crate) struct ResourceIncarnationDb {
  pub key: uuid::Uuid,
  pub resource_kind: String,
  pub resource_key: String,
  pub created_at: chrono::NaiveDateTime,
  pub deleted_at: Option<chrono::NaiveDateTime>,
  pub current_operation_key: Option<uuid::Uuid>,
}

/// Immutable accepted request and guarded terminal result. Runtime state stays
/// in the consumer's own tables; completed rows are never replaced.
#[derive(Clone, Debug, PartialEq, Queryable, Identifiable, Insertable)]
#[diesel(table_name = operations)]
#[diesel(primary_key(key))]
pub(crate) struct OperationDb {
  pub key: uuid::Uuid,
  pub incarnation_key: uuid::Uuid,
  pub resource_kind: String,
  pub resource_key: String,
  pub action: String,
  pub spec_key: uuid::Uuid,
  pub node_name: String,
  pub owner_boot_id: String,
  pub lock_device: i64,
  pub lock_inode: i64,
  pub accepted_at: chrono::NaiveDateTime,
  pub completed_at: Option<chrono::NaiveDateTime>,
  pub outcome: Option<String>,
  pub diagnostic: Option<serde_json::Value>,
}

impl OperationDb {
  pub(crate) fn owner(&self) -> OperationOwner {
    OperationOwner {
      key: self.key,
      incarnation_key: self.incarnation_key,
      resource_kind: self.resource_kind.clone(),
      resource_key: self.resource_key.clone(),
      node_name: self.node_name.clone(),
      owner_boot_id: self.owner_boot_id.clone(),
      lock_device: self.lock_device,
      lock_inode: self.lock_inode,
    }
  }

  pub(crate) fn new(
    owner: &OperationOwner,
    resource_kind: &str,
    resource_key: &str,
    spec_key: uuid::Uuid,
    action: &str,
  ) -> Self {
    Self {
      key: owner.key,
      incarnation_key: owner.incarnation_key,
      resource_kind: resource_kind.into(),
      resource_key: resource_key.into(),
      action: action.into(),
      spec_key,
      node_name: owner.node_name.clone(),
      owner_boot_id: owner.owner_boot_id.clone(),
      lock_device: owner.lock_device,
      lock_inode: owner.lock_inode,
      accepted_at: chrono::DateTime::from_timestamp_micros(
        chrono::Utc::now().timestamp_micros(),
      )
      .unwrap()
      .naive_utc(),
      completed_at: None,
      outcome: None,
      diagnostic: None,
    }
  }
}

/// Preserve database errors until the bounded transaction retry decision.
#[derive(Debug)]
pub(crate) enum OperationTransactionError {
  Database(diesel::result::Error),
  State(nanocl_error::io::IoError),
}

impl From<diesel::result::Error> for OperationTransactionError {
  fn from(error: diesel::result::Error) -> Self {
    Self::Database(error)
  }
}

impl From<nanocl_error::io::IoError> for OperationTransactionError {
  fn from(error: nanocl_error::io::IoError) -> Self {
    Self::State(error)
  }
}

impl From<OperationTransactionError> for nanocl_error::io::IoError {
  fn from(error: OperationTransactionError) -> Self {
    match error {
      OperationTransactionError::Database(error) => {
        Self::other("Operation transaction", &error.to_string())
      }
      OperationTransactionError::State(error) => error,
    }
  }
}
