use diesel::prelude::*;

use crate::schema::vm_operations;

/// Evidence for the file lock held continuously by one accepted VM mutation.
#[derive(Clone, Debug)]
pub(crate) struct VmOperationOwner {
  pub key: uuid::Uuid,
  pub node_name: String,
  pub owner_boot_id: String,
  pub lock_device: i64,
  pub lock_inode: i64,
}

/// The latest accepted operation for a VM. A NULL outcome is pending.
/// Completed records are replaced only by the next atomic acceptance.
#[derive(Clone, Debug, PartialEq, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(vm_key))]
#[diesel(table_name = vm_operations)]
pub(crate) struct VmOperationDb {
  pub vm_key: String,
  pub key: uuid::Uuid,
  pub spec_key: uuid::Uuid,
  pub node_name: String,
  pub phase: String,
  pub owner_boot_id: String,
  pub lock_device: i64,
  pub lock_inode: i64,
  pub outcome: Option<String>,
  pub observation: Option<serde_json::Value>,
}

impl VmOperationOwner {
  pub(crate) fn operation(
    &self,
    vm_key: &str,
    spec_key: uuid::Uuid,
    phase: &str,
  ) -> VmOperationDb {
    VmOperationDb {
      vm_key: vm_key.to_owned(),
      key: self.key,
      spec_key,
      node_name: self.node_name.clone(),
      phase: phase.to_owned(),
      owner_boot_id: self.owner_boot_id.clone(),
      lock_device: self.lock_device,
      lock_inode: self.lock_inode,
      outcome: None,
      observation: None,
    }
  }
}

impl VmOperationDb {
  pub(crate) fn new(
    owner: &VmOperationOwner,
    vm_key: &str,
    spec_key: uuid::Uuid,
    phase: nanocl_stubs::system::ObjPsStatusKind,
  ) -> Self {
    owner.operation(vm_key, spec_key, &phase.to_string())
  }
}

/// Preserve database errors until the transaction retry decision is made.
#[derive(Debug)]
pub(crate) enum VmOperationTransactionError {
  Database(diesel::result::Error),
  State(nanocl_error::io::IoError),
}

impl From<diesel::result::Error> for VmOperationTransactionError {
  fn from(error: diesel::result::Error) -> Self {
    Self::Database(error)
  }
}

impl From<nanocl_error::io::IoError> for VmOperationTransactionError {
  fn from(error: nanocl_error::io::IoError) -> Self {
    Self::State(error)
  }
}

impl From<VmOperationTransactionError> for nanocl_error::io::IoError {
  fn from(error: VmOperationTransactionError) -> Self {
    match error {
      VmOperationTransactionError::Database(error) => {
        Self::other("VM operation transaction", &error.to_string())
      }
      VmOperationTransactionError::State(error) => error,
    }
  }
}
