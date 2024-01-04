use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use ntex::{rt, time};
use futures::channel::mpsc;
use futures_util::{SinkExt, StreamExt};
use nanocl_error::io::{IoResult, FromIo, IoError};
use nanocl_stubs::{
  config::DaemonConfig,
  system::{Event, EventPartial},
};

use crate::{version, utils, repositories::generic::*};

use super::{Pool, EventDb, RawEventEmitter, RawEventClient};

#[derive(Debug)]
pub enum SystemEventKind {
  Emit(Event),
  Ping,
  Subscribe(SystemEventEmitter),
}

pub type SystemEventEmitter = mpsc::UnboundedSender<SystemEventKind>;
pub type SystemEventReceiver = mpsc::UnboundedReceiver<SystemEventKind>;

#[derive(Clone)]
pub struct EventManagerInner {
  /// Clients that are subscribed to the event emitter
  pub clients: Vec<SystemEventEmitter>,
}

#[derive(Clone)]
pub struct EventManager {
  /// Inner manager with system clients
  pub inner: Arc<Mutex<EventManagerInner>>,
  /// Raw emitter for http clients
  pub raw: RawEventEmitter,
  /// Emitter
  pub emitter: SystemEventEmitter,
}

impl EventManager {
  pub fn new() -> Self {
    let (sx, rx) = mpsc::unbounded();
    let inner = Arc::new(Mutex::new(EventManagerInner { clients: vec![] }));
    let n = Self {
      inner,
      emitter: sx,
      raw: RawEventEmitter::new(),
    };
    n.spawn_check_connection();
    n.run_event_loop(rx);
    n
  }

  /// Check if clients are still connected
  async fn check_connection(&mut self) {
    let mut alive_clients = Vec::new();
    let clients = self.inner.try_lock().unwrap().clients.clone();
    for mut client in clients {
      if client.send(SystemEventKind::Ping).await.is_err() {
        continue;
      }
      alive_clients.push(client.clone());
    }
    self.inner.try_lock().unwrap().clients = alive_clients;
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(&self) {
    let mut self_ptr = self.clone();
    rt::Arbiter::new().exec_fn(|| {
      rt::spawn(async move {
        let task = time::interval(Duration::from_secs(10));
        loop {
          task.tick().await;
          self_ptr.check_connection().await;
        }
      });
    });
  }

  fn dispatch_event(&self, sys_ev: SystemEventKind) -> IoResult<()> {
    log::trace!("event_manager: dispatch_event {:?}", sys_ev);
    let self_ptr = self.clone();
    match sys_ev {
      SystemEventKind::Emit(event) => {
        rt::spawn(async move {
          let clients = self_ptr.inner.try_lock().unwrap().clients.clone();
          for mut client in clients {
            let _ = client.send(SystemEventKind::Emit(event.clone())).await;
          }
          self_ptr.raw.emit(&event)?;
          Ok::<(), IoError>(())
        });
      }
      SystemEventKind::Ping => {
        log::trace!("event_manager: ping");
      }
      SystemEventKind::Subscribe(emitter) => {
        log::trace!("event_manager: subscribe");
        rt::spawn(async move {
          self_ptr.inner.try_lock().unwrap().clients.push(emitter);
          Ok::<(), IoError>(())
        });
      }
    }
    Ok(())
  }

  fn run_event_loop(&self, mut rx: SystemEventReceiver) {
    let self_ptr = self.clone();
    rt::Arbiter::new().exec_fn(move || {
      rt::spawn(async move {
        while let Some(event) = rx.next().await {
          if let Err(err) = self_ptr.dispatch_event(event) {
            log::warn!("event_manager: loop error {err}");
          }
        }
      });
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
      version: version::VERSION.to_owned(),
    };
    Ok(system_state)
  }

  pub async fn emit_event(&mut self, event: EventPartial) -> IoResult<()> {
    let event: Event = EventDb::create_try_from(event, &self.pool)
      .await?
      .try_into()?;
    self
      .event_manager
      .emitter
      .clone()
      .send(SystemEventKind::Emit(event))
      .await
      .map_err(|err| {
        IoError::interupted("EventEmitter", err.to_string().as_str())
      })?;
    Ok(())
  }

  pub fn spawn_emit_event(&self, event: EventPartial) {
    let mut self_ptr = self.clone();
    rt::spawn(async move {
      if let Err(err) = self_ptr.emit_event(event).await {
        log::warn!("system::spawn_emit_event: {err}");
      }
    });
  }

  pub async fn subscribe(&self) -> IoResult<SystemEventReceiver> {
    let (sx, rx) = mpsc::unbounded();
    self
      .event_manager
      .emitter
      .clone()
      .send(SystemEventKind::Subscribe(sx))
      .await
      .map_err(|err| {
        IoError::interupted("EventEmitter", err.to_string().as_str())
      })?;
    Ok(rx)
  }

  pub fn subscribe_raw(&self) -> IoResult<RawEventClient> {
    self.event_manager.raw.subscribe()
  }
}
