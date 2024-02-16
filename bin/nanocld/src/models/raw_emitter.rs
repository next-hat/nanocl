use std::{
  pin::Pin,
  time::Duration,
  sync::{Arc, Mutex},
  task::{Poll, Context},
};

use futures::Stream;

use ntex::{rt, time, web, util::Bytes};
use tokio::sync::mpsc::{Sender, Receiver, channel};

use nanocl_error::io::{IoResult, IoError};

use nanocl_stubs::system::{Event, EventCondition};

/// Stream: Wrap Receiver in our own type, with correct error type
/// This is needed to return a http stream of bytes
pub struct RawEventReceiver(pub Receiver<Bytes>);

impl Stream for RawEventReceiver {
  type Item = Result<Bytes, web::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    match Pin::new(&mut self.0).poll_recv(cx) {
      Poll::Ready(Some(v)) => Poll::Ready(Some(Ok(v))),
      Poll::Ready(None) => Poll::Ready(None),
      Poll::Pending => Poll::Pending,
    }
  }
}

#[derive(Clone)]
pub struct RawEventSender(
  pub Sender<Bytes>,
  pub Option<Vec<EventCondition>>,
  pub usize,
);

impl RawEventSender {
  pub fn new(
    condition: Option<Vec<EventCondition>>,
  ) -> (Self, RawEventReceiver) {
    let (tx, rx) = channel(100);
    (Self(tx, condition, 0), RawEventReceiver(rx))
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
#[derive(Clone, Default)]
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
    let mut alive_clients = Vec::new();
    let mut inner = self.inner.lock().map_err(|err| {
      IoError::interrupted("RawEmitterMutex", err.to_string().as_str())
    })?;
    for client in &inner.clients {
      if client.0.try_send(Bytes::from("")).is_err() {
        continue;
      }
      alive_clients.push(client.clone());
    }
    inner.clients = alive_clients;
    Ok(())
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(&self) {
    let mut self_ptr = self.clone();
    rt::Arbiter::new().exec_fn(|| {
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
  pub async fn emit(&self, e: &Event) -> IoResult<()> {
    let inner = Arc::clone(&self.inner);
    let clients = web::block(move || {
      let clients = inner.lock()?.clients.clone();
      Ok::<_, IoError>(clients)
    })
    .await?;
    let mut new_clients = Vec::new();
    let msg = e.try_to_bytes()?;
    for mut client in clients {
      let conditions = client.1.clone().unwrap_or_default();
      let _ = client.0.try_send(msg.clone());
      if conditions.is_empty() {
        new_clients.push(client);
      } else if conditions.iter().any(|c| c == e) {
        client.2 += 1;
        if client.2 != conditions.len() {
          new_clients.push(client);
        }
      }
    }
    let inner = Arc::clone(&self.inner);
    web::block(move || {
      let mut inner = inner.lock()?;
      inner.clients = new_clients;
      Ok::<_, IoError>(())
    })
    .await?;
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
