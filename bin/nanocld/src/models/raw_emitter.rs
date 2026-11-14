use std::{
  pin::Pin,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  task::{Context, Poll},
  time::Duration,
};

use futures::Stream;

use ntex::{rt, time, util::Bytes, web};
use tokio::sync::mpsc::{Receiver, Sender, channel, error::TrySendError};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::system::{Event, EventCondition};

/// Stream: Wrap Receiver in our own type, with correct error type
/// This is needed to return a http stream of bytes
pub struct RawEventReceiver(pub Receiver<Bytes>, Arc<AtomicBool>);

impl Stream for RawEventReceiver {
  type Item = Result<Bytes, std::io::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    match Pin::new(&mut self.0).poll_recv(cx) {
      Poll::Ready(Some(v)) => Poll::Ready(Some(Ok(v))),
      Poll::Ready(None) if self.1.swap(false, Ordering::Acquire) => {
        Poll::Ready(Some(Err(std::io::Error::new(
          std::io::ErrorKind::Interrupted,
          "Event watch buffer filled; one or more events were missed",
        ))))
      }
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }
}

struct RawEventSender {
  sender: Sender<Bytes>,
  remaining: Vec<EventCondition>,
  overflowed: Arc<AtomicBool>,
}

impl RawEventSender {
  pub fn new(
    condition: Option<Vec<EventCondition>>,
  ) -> (Self, RawEventReceiver) {
    let (tx, rx) = channel(100);
    let overflowed = Arc::new(AtomicBool::new(false));
    (
      Self {
        sender: tx,
        remaining: condition.unwrap_or_default(),
        overflowed: Arc::clone(&overflowed),
      },
      RawEventReceiver(rx, overflowed),
    )
  }
}

/// Trait to convert a type to bytes
trait TryToBytes {
  type Error;

  fn try_to_bytes(&self) -> Result<Bytes, Self::Error>;
}

/// Convert event to bytes to send to clients
impl TryToBytes for Event {
  type Error = IoError;

  fn try_to_bytes(&self) -> Result<Bytes, Self::Error> {
    let mut data = serde_json::to_vec(&self)?;
    data.push(b'\n');
    Ok(Bytes::from(data))
  }
}

/// Raw event emitter
#[derive(Clone, Default)]
pub struct RawEventEmitter {
  inner: Arc<Mutex<RawEventEmitterInner>>,
}

/// Inner struct for raw event emitter
/// Contains a list of clients
#[derive(Default)]
pub struct RawEventEmitterInner {
  clients: Vec<RawEventSender>,
}

impl RawEventEmitter {
  pub fn new() -> Self {
    let self_ptr = Self {
      inner: Arc::new(Mutex::new(RawEventEmitterInner { clients: vec![] })),
    };
    self_ptr.spawn_check_connection();
    self_ptr
  }

  /// Check if clients are still connected
  fn check_connection(&mut self) -> IoResult<()> {
    let mut inner = self.inner.lock().map_err(|err| {
      IoError::interrupted("RawEmitterMutex", err.to_string().as_str())
    })?;
    inner.clients.retain(|client| {
      // A queued event or heartbeat already gives the HTTP writer data to poll.
      if client.sender.capacity() != client.sender.max_capacity() {
        return !client.sender.is_closed();
      }
      !matches!(
        client.sender.try_send(Bytes::new()),
        Err(TrySendError::Closed(_))
      )
    });
    Ok(())
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(&self) {
    let mut self_ptr = self.clone();
    rt::Arbiter::new().handle().spawn(async move {
      rt::spawn(async move {
        let task = time::interval(Duration::from_secs(10));
        loop {
          task.tick().await;
          if let Err(err) = self_ptr.check_connection() {
            log::error!("{err}");
          }
        }
      });
    });
  }

  /// Send an event to all clients
  pub fn emit(&self, e: &Event) -> IoResult<()> {
    let msg = e.try_to_bytes()?;
    // Acceptance notifications and the system event loop can publish from
    // different arbiters. Keep delivery and condition progress atomic with
    // subscriptions and connection cleanup; try_send never blocks.
    let mut inner = self.inner.lock()?;
    inner.clients.retain_mut(|client| {
      match client.sender.try_send(msg.clone()) {
        Err(TrySendError::Closed(_)) => return false,
        Err(TrySendError::Full(_)) => {
          // The receiver reports the gap after draining the contiguous prefix.
          client.overflowed.store(true, Ordering::Release);
          return false;
        }
        Ok(()) => {}
      }
      if client.remaining.is_empty() {
        return true;
      }
      client.remaining.retain(|condition| condition != e);
      !client.remaining.is_empty()
    });
    Ok(())
  }

  /// Subscribe to events
  pub async fn subscribe(
    &self,
    condition: Option<Vec<EventCondition>>,
  ) -> IoResult<RawEventReceiver> {
    let (tx, rx) = RawEventSender::new(condition);
    let inner = Arc::clone(&self.inner);
    web::block(move || {
      inner.lock()?.clients.push(tx);
      Ok::<_, IoError>(())
    })
    .await?;
    Ok(rx)
  }
}

#[cfg(test)]
mod tests {
  use futures::{FutureExt, StreamExt};
  use nanocl_stubs::system::{
    EventActor, EventActorKind, EventKind, NativeEventAction,
  };

  use super::*;

  fn event(action: &str) -> Event {
    Event {
      key: uuid::Uuid::nil(),
      created_at: Default::default(),
      expires_at: Default::default(),
      reporting_node: "test".into(),
      reporting_controller: "nanocl.io/core".into(),
      kind: EventKind::Normal,
      action: action.into(),
      reason: "state_sync".into(),
      note: None,
      actor: None,
      related: None,
      metadata: None,
    }
  }

  fn subscribe(
    emitter: &RawEventEmitter,
    conditions: Vec<EventCondition>,
  ) -> RawEventReceiver {
    let (client, receiver) = RawEventSender::new(Some(conditions));
    emitter.inner.lock().unwrap().clients.push(client);
    receiver
  }

  #[test]
  fn watch_closes_after_all_conditions_have_matched() {
    let emitter = RawEventEmitter::default();
    let first = EventCondition {
      kind: vec![EventKind::Normal],
      action: vec![NativeEventAction::Start],
      ..Default::default()
    };
    let second = EventCondition {
      actor_kind: Some(EventActorKind::Process),
      related_key: Some("global.web".into()),
      related_kind: Some(EventActorKind::Cargo),
      ..first.clone()
    };
    let mut stream = subscribe(&emitter, vec![second.clone(), first, second]);
    let mut start = event("start");
    start.actor = Some(EventActor {
      key: None,
      kind: EventActorKind::Process,
      attributes: None,
    });
    emitter.emit(&start).unwrap();
    emitter.emit(&start).unwrap();
    emitter.emit(&start).unwrap();
    start.related = Some(EventActor {
      key: Some("global.web".into()),
      kind: EventActorKind::Cargo,
      attributes: None,
    });
    emitter.emit(&start).unwrap();
    for related in [false, false, false, true] {
      let bytes = stream.0.try_recv().unwrap();
      assert_eq!(
        serde_json::from_slice::<Event>(&bytes)
          .unwrap()
          .related
          .is_some(),
        related
      );
    }
    assert!(matches!(stream.next().now_or_never(), Some(None)));
  }

  #[test]
  fn watch_reports_overflow_after_delivering_queued_events() {
    let mut emitter = RawEventEmitter::default();
    let mut slow = subscribe(&emitter, vec![]);
    let mut fast = subscribe(&emitter, vec![]);
    let capacity = slow.0.max_capacity();
    let start = event("start");
    for _ in 0..capacity {
      emitter.emit(&start).unwrap();
      fast.0.try_recv().unwrap();
    }
    // A heartbeat does not drop a subscriber whose queue holds events.
    emitter.check_connection().unwrap();
    emitter.emit(&event("stop")).unwrap();
    for _ in 0..capacity {
      let bytes = slow.0.try_recv().unwrap();
      assert_eq!(
        serde_json::from_slice::<Event>(&bytes).unwrap().action,
        "start"
      );
    }
    let error = slow.next().now_or_never().unwrap().unwrap().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert!(matches!(slow.next().now_or_never(), Some(None)));
    // A slow subscriber does not interrupt other watchers.
    assert!(fast.0.try_recv().unwrap().is_empty());
    let bytes = fast.0.try_recv().unwrap();
    assert_eq!(
      serde_json::from_slice::<Event>(&bytes).unwrap().action,
      "stop"
    );
    assert!(fast.next().now_or_never().is_none());
  }
}
