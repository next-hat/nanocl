use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateResource, StateVirtualMachine, StateSecret,
  StateJob,
};

use crate::event::EventEmitter;

use super::Pool;

/// ## DaemonState
///
/// This structure represent the state of the daemon.
/// Used to share the state between the different handlers.
/// It contains the database connection pool, the docker client, the config and the event emitter.
///
#[derive(Clone)]
pub struct DaemonState {
  /// The database connection pool
  pub pool: Pool,
  /// The docker client
  pub docker_api: bollard_next::Docker,
  /// The config of the daemon
  pub config: DaemonConfig,
  /// The event emitter
  pub event_emitter: EventEmitter,
  /// Latest version of the daemon or version of current request
  #[allow(dead_code)]
  pub version: String,
}

/// ## StateData
///
/// This enum represent the different `Statefile` format that can be parsed.
///
#[derive(Clone, Debug)]
pub enum StateFileData {
  Deployment(StateDeployment),
  Cargo(StateCargo),
  VirtualMachine(StateVirtualMachine),
  Resource(StateResource),
  Secret(StateSecret),
  Job(StateJob),
}
