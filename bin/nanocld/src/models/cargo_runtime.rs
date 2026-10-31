use std::collections::HashMap;

use nanocl_error::io::IoError;
use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::{Config as DockerConfig, ContainerSpec},
  node::CargoReplicaTask,
  process::Process,
};

use super::{
  CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
  CargoRuntimeContext, SystemState,
};

/// Cleanup evidence is limited to candidate IDs returned by local creation.
/// An uncertain create may still have produced another, untracked runtime.
pub(crate) struct CargoCandidateFailure {
  pub(crate) error: IoError,
  pub(crate) tracked_candidate_removed: bool,
}

impl From<IoError> for CargoCandidateFailure {
  fn from(error: IoError) -> Self {
    Self {
      error,
      tracked_candidate_removed: false,
    }
  }
}

/// Operation-local evidence captured before a replica rollout mutates runtime.
#[derive(Clone, Debug)]
pub(crate) enum CargoPreviousRuntime {
  /// No attachment, persisted process, or Docker runtime belongs to this replica.
  Absent,
  /// Immutable process IDs and their positively observed running state.
  Observed(HashMap<String, bool>),
}

pub(crate) type ProcessesByKey<'a> = HashMap<&'a str, &'a Process>;
pub(crate) type CargoStableProcesses<'a> =
  HashMap<(uuid::Uuid, CargoReplicaProcessRole, String), Vec<&'a Process>>;

pub(crate) struct CompileDeclaredOpts<'a> {
  pub(crate) cargo: &'a Cargo,
  pub(crate) task: &'a CargoReplicaTask,
  pub(crate) declared: &'a ContainerSpec,
  pub(crate) role: CargoReplicaProcessRole,
  pub(crate) sandbox_id: Option<&'a str>,
  pub(crate) attempt_id: &'a str,
  pub(crate) gateway: &'a str,
  pub(crate) state: &'a SystemState,
}

pub(crate) struct EnsureExistingSlotOpts<'a> {
  pub(crate) cargo: &'a Cargo,
  pub(crate) task: &'a CargoReplicaTask,
  pub(crate) mapping: CargoReplicaProcessDb,
  pub(crate) role: CargoReplicaProcessRole,
  pub(crate) container_name: &'a str,
  pub(crate) essential: bool,
  pub(crate) config: &'a DockerConfig,
  pub(crate) state: &'a CargoRuntimeContext,
}

pub(crate) struct ValidateRestartSlotOpts<'opts, 'process> {
  pub(crate) cargo_key: &'opts str,
  pub(crate) replica: &'opts CargoReplicaDb,
  pub(crate) mapping: &'opts CargoReplicaProcessDb,
  pub(crate) expected_role: CargoReplicaProcessRole,
  pub(crate) expected_name: &'opts str,
  pub(crate) expected_essential: bool,
  pub(crate) processes_by_key: &'opts ProcessesByKey<'process>,
  pub(crate) stable_identities: &'opts CargoStableProcesses<'process>,
}
