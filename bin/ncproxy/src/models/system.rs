use std::sync::Arc;

use futures::{channel::mpsc, SinkExt, StreamExt};
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
}

pub type SystemStateRef = Arc<SystemState>;

/// Type of event that can be emitted
pub enum SystemEventKind {
  Reload,
}

struct SystemEventInner {
  client: NanocldClient,
  task: ntex::rt::JoinHandle<IoResult<()>>,
}

pub struct SystemEvent(SystemEventInner);

impl SystemEvent {
  pub fn new(client: &NanocldClient) -> Self {
    Self(SystemEventInner {
      client: client.clone(),
      task: rt::spawn(async move { Ok::<_, IoError>(()) }),
    })
  }

  pub fn handle(&mut self, _e: SystemEventKind) {
    let abort_handle = self.0.task.abort_handle();
    if !abort_handle.is_finished() {
      log::info!("system: aborting reload task");
      abort_handle.abort();
    }
    let client = self.0.client.clone();
    self.0.task = rt::spawn(async move {
      ntex::time::sleep(std::time::Duration::from_millis(750)).await;
      if let Err(err) = utils::nginx::reload(&client).await {
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
  pub fn new(client: &NanocldClient) -> Self {
    let (tx, mut rx) = mpsc::unbounded();
    let client = client.clone();
    rt::Arbiter::new().exec_fn(move || {
      ntex::rt::spawn(async move {
        let mut local_event = SystemEvent::new(&client);
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
