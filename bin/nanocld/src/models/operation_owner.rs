use std::{fs::File, sync::Arc};

use serde::{Deserialize, Serialize};

/// Immutable identity written before an acceptance may reach the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationOwner {
  pub key: uuid::Uuid,
  pub resource_kind: String,
  pub resource_key: String,
  pub incarnation_key: uuid::Uuid,
  pub node_name: String,
  pub owner_boot_id: String,
  pub lock_device: i64,
  pub lock_inode: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationOwnerFile {
  pub version: u8,
  pub owner: OperationOwner,
}

/// A stable state-directory gate. Its pathname is never removed by maintenance.
#[derive(Clone)]
pub(crate) struct StateDirectoryGuard {
  pub(crate) _file: Arc<File>,
  pub(crate) root: Arc<File>,
  pub(crate) exclusive: bool,
}

/// Clones retain the same flock and the daemon/maintenance gate. Database
/// workers must retain a clone until their actual transaction has finished.
#[derive(Clone)]
pub(crate) struct OperationGuard {
  pub(crate) _file: Arc<File>,
  pub(crate) directory: Arc<File>,
  pub(crate) owner: OperationOwner,
  pub(crate) identity_verified: bool,
  pub(crate) _gate: StateDirectoryGuard,
}

/// Keep enumeration progress across passes so busy early entries cannot starve
/// eligible files later in the directory. No operation history is discarded.
pub(crate) struct OperationCleanupCursor {
  pub(crate) directory: Arc<File>,
  pub(crate) entries: std::fs::ReadDir,
  pub(crate) gate: StateDirectoryGuard,
}
