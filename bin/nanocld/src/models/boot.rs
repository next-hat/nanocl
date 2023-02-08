use nanocl_stubs::config::DaemonConfig;

use crate::event::EventEmitterPtr;

use super::Pool;

/// The structure returned by the boot process
pub struct BootState {
  /// The database connection pool
  pub(crate) pool: Pool,
  /// The docker client
  pub(crate) docker_api: bollard_next::Docker,
  /// The config of the daemon
  pub(crate) config: DaemonConfig,
  /// The event emitter
  pub(crate) event_emitter: EventEmitterPtr,
}
