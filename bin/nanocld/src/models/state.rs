use nanocl_stubs::config::DaemonConfig;

use crate::event_emitter::EventEmitter;

use super::Pool;

/// This structure represent the state of the daemon.
/// Used to share the state between the different handlers.
/// It contains the database connection pool, the docker client, the config and the event emitter.
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
  /// Latest version of the daemon
  #[allow(dead_code)]
  pub version: String,
}
