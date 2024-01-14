use std::sync::Arc;

use ntex::rt;
use nanocl_error::{
  io::{IoResult, FromIo, IoError},
  http::HttpResult,
};
use nanocl_stubs::{
  config::DaemonConfig,
  system::{Event, EventPartial, NativeEventAction, EventActor, EventKind},
};

use crate::{vars, utils, repositories::generic::*, objects::generic::StateAction};

use super::{Pool, EventDb, RawEventEmitter, RawEventClient, TaskManager};

#[derive(Clone)]
pub struct EventManager {
  /// Raw emitter for http clients
  pub raw: RawEventEmitter,
}

impl Default for EventManager {
  fn default() -> Self {
    Self::new()
  }
}

impl EventManager {
  pub fn new() -> Self {
    Self {
      raw: RawEventEmitter::new(),
    }
  }

  fn dispatch_event(&self, ev: Event) {
    log::trace!("event_manager: dispatch_event {:?}", ev);
    let self_ptr = self.clone();
    rt::spawn(async move {
      self_ptr.raw.emit(&ev).await?;
      Ok::<(), IoError>(())
    });
  }
}

/// This structure represent the state of the system.
/// Used to share the state between the different handlers.
/// It contains the database connection pool, the docker client, the config and the event emitter.
#[derive(Clone)]
pub struct SystemState {
  /// The database connection pool
  pub pool: Pool,
  /// The docker client
  pub docker_api: bollard_next::Docker,
  /// The config of the daemon
  pub config: DaemonConfig,
  /// Event manager that run the event loop
  pub event_manager: EventManager,
  /// Manager of the tasks
  pub task_manager: TaskManager,
  /// Latest version of the daemon
  pub version: String,
}

impl SystemState {
  /// Create a new instance of the system state
  /// It will create the database connection pool and the docker client
  /// and the event emitter
  pub async fn new(conf: &DaemonConfig) -> IoResult<Self> {
    let docker = bollard_next::Docker::connect_with_unix(
      &conf.docker_host,
      120,
      bollard_next::API_DEFAULT_VERSION,
    )
    .map_err(|err| err.map_err_context(|| "Docker"))?;
    let pool = utils::store::init(conf).await?;
    let system_state = SystemState {
      pool: Arc::clone(&pool),
      docker_api: docker.clone(),
      config: conf.to_owned(),
      event_manager: EventManager::new(),
      task_manager: TaskManager::new(),
      version: vars::VERSION.to_owned(),
    };
    Ok(system_state)
  }

  pub async fn exec_action<A>(&self, action: A) -> HttpResult<A::StateActionOut>
  where
    A: StateAction,
  {
    action.fn_action(self).await
  }

  pub async fn emit_event(&self, new_ev: EventPartial) -> IoResult<()> {
    let ev: Event = EventDb::create_try_from(new_ev, &self.pool)
      .await?
      .try_into()?;
    crate::subsystem::exec_event(&ev, self).await?;
    self.event_manager.dispatch_event(ev);
    Ok(())
  }

  pub fn spawn_emit_event(&self, event: EventPartial) {
    let self_ptr = self.clone();
    rt::spawn(async move {
      if let Err(err) = self_ptr.emit_event(event).await {
        log::warn!("system::spawn_emit_event: {err}");
      }
    });
  }

  pub async fn subscribe_raw(&self) -> IoResult<RawEventClient> {
    self.event_manager.raw.subscribe().await
  }

  pub fn emit_normal_native_action<A>(
    &self,
    actor: &A,
    action: NativeEventAction,
  ) where
    A: Into<EventActor> + Clone,
  {
    let actor = actor.clone().into();
    let event = EventPartial {
      reporting_controller: vars::CONTROLLER_NAME.to_owned(),
      reporting_node: self.config.hostname.clone(),
      kind: EventKind::Normal,
      action: action.to_string(),
      related: None,
      reason: "state_sync".to_owned(),
      note: None,
      metadata: None,
      actor: Some(actor),
    };
    self.spawn_emit_event(event);
  }
}
