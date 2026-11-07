use std::{
  future::Future,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::Duration,
};

use futures::{SinkExt, StreamExt, channel::mpsc, lock::Mutex};
use ntex::rt;

use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

use crate::utils;

use super::{AcceptedRules, Store};

/// Shared state of the program
#[derive(Clone)]
pub struct SystemState {
  pub store: Store,
  pub client: NanocldClient,
  pub event_emitter: EventEmitter,
  pub nginx_dir: String,
  /// Serializes every nginx configuration mutation and validation cycle.
  pub config_lock: Arc<Mutex<()>>,
  /// Briefly held for declaration snapshots/publication. Config writers acquire
  /// config_lock first; this guard must never span network or nginx operations.
  pub(crate) accepted_rules: Arc<Mutex<AcceptedRules>>,
  /// Holds both controller admission locks until the process shuts down.
  pub _controller_locks: Option<Arc<(std::fs::File, std::fs::File)>>,
}

pub type SystemStateRef = Arc<SystemState>;

/// Type of event that can be emitted
pub enum SystemEventKind {
  Reload,
}

struct SystemEventInner {
  store: Store,
  config_lock: Arc<Mutex<()>>,
  task: Option<ntex::rt::JoinHandle<IoResult<()>>>,
  requested: Arc<AtomicU64>,
}

pub struct SystemEvent(SystemEventInner);

async fn reload_loop<F, R, W, S>(
  requested: &AtomicU64,
  debounce: Duration,
  mut reload: F,
  mut wait: W,
) -> IoResult<()>
where
  F: FnMut() -> R,
  R: Future<Output = IoResult<()>>,
  W: FnMut(Duration) -> S,
  S: Future<Output = ()>,
{
  let mut revision = requested.load(Ordering::Relaxed);
  let mut delay = debounce;
  loop {
    let mut remaining = delay;
    loop {
      // Observe new requests at the debounce cadence without acquiring the
      // config lock or leaving a newer configuration behind a long retry wait.
      let step = remaining.min(debounce);
      wait(step).await;
      let latest = requested.load(Ordering::Relaxed);
      if latest != revision {
        revision = latest;
        delay = debounce;
        remaining = debounce;
        continue;
      }
      remaining = remaining.saturating_sub(step);
      if remaining.is_zero() {
        break;
      }
    }
    let result = reload().await;
    let latest = requested.load(Ordering::Relaxed);
    match result {
      Ok(()) if latest == revision => return Ok(()),
      Ok(()) => delay = debounce,
      Err(err) => {
        log::warn!("system: {err}");
        delay = if latest == revision {
          delay.saturating_mul(2).min(Duration::from_secs(30))
        } else {
          debounce
        };
      }
    }
    revision = latest;
  }
}

impl SystemEvent {
  pub fn new(store: &Store, config_lock: Arc<Mutex<()>>) -> Self {
    Self(SystemEventInner {
      store: store.clone(),
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
    let store = self.0.store.clone();
    let config_lock = Arc::clone(&self.0.config_lock);
    let requested = Arc::clone(&self.0.requested);
    self.0.task = Some(rt::spawn(async move {
      reload_loop(
        &requested,
        Duration::from_millis(750),
        || async {
          let _guard = config_lock.lock().await;
          // Once work starts, keep its lock until the bounded reload ends.
          // Later events request a follow-up instead of cancelling a reload.
          utils::nginx::reload(&store).await
        },
        ntex::time::sleep,
      )
      .await
    }));
  }
}

#[derive(Clone)]
pub struct EventEmitter(pub Arc<mpsc::UnboundedSender<SystemEventKind>>);

impl EventEmitter {
  /// Create a new thread with it's own event loop and return an emitter to send events to it
  pub fn new(store: &Store, config_lock: Arc<Mutex<()>>) -> Self {
    let (tx, mut rx) = mpsc::unbounded();
    let store = store.clone();
    rt::Arbiter::new().handle().spawn(async move {
      rt::spawn(async move {
        let mut local_event = SystemEvent::new(&store, config_lock);
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
  use nanocl_error::io::IoError;

  use super::*;

  #[ntex::test]
  async fn failed_reload_retries_without_another_event() {
    let requested = AtomicU64::new(1);
    let attempts = std::cell::Cell::new(0);
    reload_loop(
      &requested,
      Duration::ZERO,
      || {
        let attempt = attempts.get();
        attempts.set(attempt + 1);
        futures::future::ready(if attempt == 0 {
          Err(IoError::other("Reload", "temporarily unavailable"))
        } else {
          Ok(())
        })
      },
      ntex::time::sleep,
    )
    .await
    .unwrap();
    assert_eq!(attempts.get(), 2);
  }

  #[ntex::test]
  async fn reload_retries_when_a_new_request_arrives_during_reload() {
    let requested = AtomicU64::new(1);
    let attempts = std::cell::Cell::new(0);
    reload_loop(
      &requested,
      Duration::ZERO,
      || {
        if attempts.get() == 0 {
          requested.fetch_add(1, Ordering::Relaxed);
        }
        attempts.set(attempts.get() + 1);
        futures::future::ready(Ok(()))
      },
      ntex::time::sleep,
    )
    .await
    .unwrap();
    assert_eq!(attempts.get(), 2);
  }

  #[ntex::test]
  async fn reload_failure_backoff_is_capped_and_success_resets_it() {
    let requested = AtomicU64::new(1);
    let waited = std::cell::Cell::new(Duration::ZERO);
    let delays = std::cell::RefCell::new(Vec::new());
    reload_loop(
      &requested,
      Duration::from_millis(750),
      || {
        let mut delays = delays.borrow_mut();
        delays.push(waited.replace(Duration::ZERO));
        let result = if delays.len() <= 8 {
          Err(IoError::other("Reload", "temporarily unavailable"))
        } else {
          if delays.len() == 9 {
            requested.fetch_add(1, Ordering::Relaxed);
          }
          Ok(())
        };
        futures::future::ready(result)
      },
      |delay| {
        waited.set(waited.get() + delay);
        futures::future::ready(())
      },
    )
    .await
    .unwrap();
    assert_eq!(
      *delays.borrow(),
      [
        750, 1500, 3000, 6000, 12000, 24000, 30000, 30000, 30000, 750
      ]
      .map(Duration::from_millis)
    );
  }

  #[ntex::test]
  async fn new_reload_request_resets_backoff_and_debounce() {
    let requested = AtomicU64::new(1);
    let waited = std::cell::Cell::new(Duration::ZERO);
    let delays = std::cell::RefCell::new(Vec::new());
    reload_loop(
      &requested,
      Duration::from_millis(750),
      || {
        let mut delays = delays.borrow_mut();
        delays.push(waited.replace(Duration::ZERO));
        futures::future::ready(if delays.len() <= 3 {
          Err(IoError::other("Reload", "temporarily unavailable"))
        } else {
          Ok(())
        })
      },
      |delay| {
        waited.set(waited.get() + delay);
        if delays.borrow().len() == 3 && requested.load(Ordering::Relaxed) == 1
        {
          requested.fetch_add(1, Ordering::Relaxed);
        }
        futures::future::ready(())
      },
    )
    .await
    .unwrap();
    // The new request arrives one base interval into a six-second retry wait.
    // It gets a complete fresh debounce, then runs without the old backoff.
    assert_eq!(
      *delays.borrow(),
      [750, 1500, 3000, 1500].map(Duration::from_millis)
    );
    assert_eq!(requested.load(Ordering::Relaxed), 2);
  }

  #[ntex::test]
  async fn reload_events_preserve_an_active_task_and_record_follow_up() {
    let mut event =
      SystemEvent::new(&Store::isolated("unused"), Arc::new(Mutex::new(())));
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
