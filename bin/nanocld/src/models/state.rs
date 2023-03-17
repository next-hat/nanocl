use nanocl_stubs::{
  config::DaemonConfig,
  state::{StateDeployment, StateCargo, StateResources},
};

use crate::event::EventEmitterPtr;

use super::Pool;

#[derive(Clone)]
pub struct DaemonState {
  /// The database connection pool
  pub(crate) pool: Pool,
  /// The docker client
  pub(crate) docker_api: bollard_next::Docker,
  /// The config of the daemon
  pub(crate) config: DaemonConfig,
  /// The event emitter
  pub(crate) event_emitter: EventEmitterPtr,
}

#[derive(Debug)]
pub enum StateData {
  Deployment(StateDeployment),
  Cargo(StateCargo),
  Resource(StateResources),
}
