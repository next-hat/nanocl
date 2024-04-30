use std::sync::Arc;

use ntex::rt;
use futures::channel::mpsc;
use futures_util::{SinkExt, StreamExt};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocl_stubs::{
  config::DaemonConfig,
  system::{
    Event, EventActor, EventKind, EventPartial, EventCondition,
    NativeEventAction,
  },
};

use crate::{
  vars, utils,
  repositories::generic::*,
  models::{
    EventDb, RawEventEmitter, RawEventReceiver, SystemState, SystemStateInner,
    TaskManager,
  },
};

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
    let (sx, rx) = mpsc::unbounded();
    let system_state = SystemState {
      inner: Arc::new(SystemStateInner {
        pool: Arc::clone(&pool),
        docker_api: docker.clone(),
        config: conf.to_owned(),
        event_emitter: sx,
        event_emitter_raw: RawEventEmitter::new(),
        task_manager: TaskManager::new(),
        version: vars::VERSION.to_owned(),
        arbiter: rt::Arbiter::new(),
      }),
    };
    system_state.clone().run(rx);
    Ok(system_state)
  }

  /// Start the system event loop
  fn run(self, mut rx: mpsc::UnboundedReceiver<Event>) {
    self.inner.arbiter.clone().exec_fn(move || {
      rt::spawn(async move {
        while let Some(e) = rx.next().await {
          if let Err(err) = self.inner.event_emitter_raw.emit(&e) {
            log::error!("system::run: raw emit {err}");
          }
          if let Err(err) = super::exec_event(&e, &self).await {
            log::error!("system::run: exec event {err}");
          }
        }
        Ok::<(), IoError>(())
      });
    });
  }

  /// Emit an event to the system event loop
  pub async fn emit_event(&self, new_ev: EventPartial) -> IoResult<()> {
    let ev: Event = EventDb::create_try_from(new_ev, &self.inner.pool)
      .await?
      .try_into()?;
    self
      .inner
      .event_emitter
      .clone()
      .send(ev)
      .await
      .map_err(|err| {
        IoError::interrupted("Event Emitter", err.to_string().as_str())
      })?;
    Ok(())
  }

  /// Emit an event in the background to the system event loop
  pub fn spawn_emit_event(&self, event: EventPartial) {
    let self_ptr = self.clone();
    rt::spawn(async move {
      if let Err(err) = self_ptr.emit_event(event).await {
        log::warn!("system::spawn_emit_event: {err}");
      }
    });
  }

  /// Subscribe an http client to the event loop
  pub async fn subscribe_raw(
    &self,
    condition: Option<Vec<EventCondition>>,
  ) -> IoResult<RawEventReceiver> {
    self.inner.event_emitter_raw.subscribe(condition).await
  }

  /// Emit a Error event action
  pub fn emit_error_native_action<A>(
    &self,
    actor: &A,
    action: NativeEventAction,
    note: Option<String>,
  ) where
    A: Into<EventActor> + Clone,
  {
    let actor = actor.clone().into();
    let event = EventPartial {
      reporting_controller: vars::CONTROLLER_NAME.to_owned(),
      reporting_node: self.inner.config.hostname.clone(),
      kind: EventKind::Error,
      action: action.to_string(),
      related: None,
      reason: "state_sync".to_owned(),
      note: match note {
        None => Some(format!(
          "{} {}",
          actor.kind,
          actor.key.clone().unwrap_or_default()
        )),
        Some(note) => Some(note),
      },
      metadata: None,
      actor: Some(actor),
    };
    self.spawn_emit_event(event);
  }

  /// Emit a normal event action
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
      reporting_node: self.inner.config.hostname.clone(),
      kind: EventKind::Normal,
      action: action.to_string(),
      related: None,
      reason: "state_sync".to_owned(),
      note: Some(format!(
        "{} {}",
        actor.kind,
        actor.key.clone().unwrap_or_default()
      )),
      metadata: None,
      actor: Some(actor),
    };
    self.spawn_emit_event(event);
  }

  /// Wait for the event loop to finish
  pub async fn wait_event_loop(&self) {
    self.inner.event_emitter.clone().flush().await.unwrap();
    self.inner.arbiter.clone().join().unwrap();
  }
}
