use std::sync::{
  Arc,
  atomic::{AtomicU64, Ordering},
};

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
  task: Option<ntex::rt::JoinHandle<IoResult<()>>>,
  requested: Arc<AtomicU64>,
}

pub struct SystemEvent(SystemEventInner);

impl SystemEvent {
  pub fn new(state_dir: &str, config_lock: Arc<Mutex<()>>) -> Self {
    Self(SystemEventInner {
      state_dir: state_dir.to_owned(),
      config_lock,
      task: None,
      requested: Arc::new(AtomicU64::new(0)),
    })
  }

  pub fn handle(&mut self, _e: SystemEventKind) {
    self.0.requested.fetch_add(1, Ordering::Relaxed);
    if self.0.task.as_ref().is_some_and(|task| !task.is_finished()) {
      return;
    }
    let state_dir = self.0.state_dir.clone();
    let config_lock = Arc::clone(&self.0.config_lock);
    let requested = Arc::clone(&self.0.requested);
    self.0.task = Some(rt::spawn(async move {
      loop {
        let revision = requested.load(Ordering::Relaxed);
        ntex::time::sleep(std::time::Duration::from_millis(750)).await;
        if requested.load(Ordering::Relaxed) != revision {
          continue;
        }
        {
          let _guard = config_lock.lock().await;
          // Once work starts, keep its lock until the bounded reload ends.
          // Later events request a follow-up instead of cancelling a reload.
          if let Err(err) = utils::nginx::reload(&state_dir).await {
            log::warn!("system: {err}");
          }
        }
        if requested.load(Ordering::Relaxed) == revision {
          return Ok::<_, IoError>(());
        }
      }
    }));
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

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn reload_events_preserve_an_active_task_and_record_follow_up() {
    let mut event = SystemEvent::new("unused", Arc::new(Mutex::new(())));
    assert!(event.0.task.is_none());
    let (release, wait) = futures::channel::oneshot::channel::<()>();
    let (finished, completion) = futures::channel::oneshot::channel::<()>();
    event.0.task = Some(rt::spawn(async move {
      wait.await.unwrap();
      finished.send(()).unwrap();
      Ok(())
    }));
    event.handle(SystemEventKind::Reload);
    event.handle(SystemEventKind::Reload);
    assert_eq!(event.0.requested.load(Ordering::Relaxed), 2);
    release.send(()).unwrap();
    // Cancelling the active task would drop its sender and fail this await.
    completion.await.unwrap();
  }
}
