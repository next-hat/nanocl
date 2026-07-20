use std::sync::Arc;

use futures::{SinkExt, StreamExt, channel::mpsc, lock::Mutex};
use ntex::rt;

use nanocl_error::io::{IoError, IoResult};
use nanocld_client::NanocldClient;

use crate::utils;

use super::Store;

/// Shared state of the program
#[derive(Clone)]
pub struct SystemState {
  pub store: Store,
  pub client: NanocldClient,
  pub event_emitter: EventEmitter,
  pub nginx_dir: String,
  /// Serializes every nginx configuration mutation and validation cycle.
  pub config_lock: Arc<Mutex<()>>,
}

pub type SystemStateRef = Arc<SystemState>;

/// Type of event that can be emitted
pub enum SystemEventKind {
  Reload,
}

struct SystemEventInner {
  state_dir: String,
  config_lock: Arc<Mutex<()>>,
  task: ntex::rt::JoinHandle<IoResult<()>>,
}

pub struct SystemEvent(SystemEventInner);

impl SystemEvent {
  pub fn new(state_dir: &str, config_lock: Arc<Mutex<()>>) -> Self {
    Self(SystemEventInner {
      state_dir: state_dir.to_owned(),
      config_lock,
      task: rt::spawn(async move { Ok::<_, IoError>(()) }),
    })
  }

  pub fn handle(&mut self, _e: SystemEventKind) {
    if !self.0.task.is_finished() {
      log::info!("system: canceling reload task");
      let task = std::mem::replace(
        &mut self.0.task,
        rt::spawn(async move { Ok::<_, IoError>(()) }),
      );
      task.cancel();
    }
    let state_dir = self.0.state_dir.clone();
    let config_lock = Arc::clone(&self.0.config_lock);
    self.0.task = rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_millis(750)).await;
      let _guard = config_lock.lock().await;
      if let Err(err) = utils::nginx::reload(&state_dir).await {
        log::warn!("system: {err}");
      }
      Ok::<_, IoError>(())
    });
  }
}

#[derive(Clone)]
pub struct EventEmitter(pub Arc<mpsc::UnboundedSender<SystemEventKind>>);

impl EventEmitter {
  /// Create a new thread with it's own event loop and return an emitter to send events to it
  pub fn new(state_dir: &str, config_lock: Arc<Mutex<()>>) -> Self {
    let (tx, mut rx) = mpsc::unbounded();
    let state_dir = state_dir.to_owned();
    rt::Arbiter::new().handle().spawn(async move {
      rt::spawn(async move {
        let mut local_event = SystemEvent::new(&state_dir, config_lock);
        while let Some(e) = rx.next().await {
          local_event.handle(e);
        }
      });
    });
    Self(Arc::new(tx))
  }

  pub async fn emit(&self, event: SystemEventKind) {
    let emiter = Arc::clone(&self.0);
    if let Err(err) = emiter.as_ref().send(event).await {
      log::error!("Unable to emit event: {err}");
    }
  }

  pub async fn emit_reload(&self) {
    self.emit(SystemEventKind::Reload).await;
  }
}
