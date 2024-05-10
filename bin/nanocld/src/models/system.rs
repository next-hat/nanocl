use std::sync::Arc;

use ntex::rt;
use futures::channel::mpsc;

use nanocl_stubs::{config::DaemonConfig, system::Event};

use super::{Pool, RawEventEmitter, TaskManager};

/// This structure represent the state of the system.
/// Used to share the state between the different handlers.
/// It contains the database connection pool, the docker client, the config and the event emitter.
pub struct SystemStateInner {
  /// The database connection pool
  pub pool: Pool,
  /// The docker client
  pub docker_api: bollard_next::Docker,
  /// The config of the daemon
  pub config: DaemonConfig,
  /// Manager of the tasks
  pub task_manager: TaskManager,
  /// Event emitter
  pub(crate) event_emitter: mpsc::UnboundedSender<Event>,
  /// Http event client
  pub(crate) event_emitter_raw: RawEventEmitter,
  /// task event loop
  pub(crate) arbiter: rt::Arbiter,
}

#[derive(Clone)]
pub struct SystemState {
  pub inner: Arc<SystemStateInner>,
}
