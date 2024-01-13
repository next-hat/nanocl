use std::{
  pin::Pin,
  time::Duration,
  sync::{Arc, Mutex},
  task::{Poll, Context},
};

use nanocl_stubs::system::Event;
use ntex::{rt, web, time, util::Bytes};
use futures::Stream;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_error::io::{IoResult, IoError};

/// Stream: Wrap Receiver in our own type, with correct error type
/// This is needed to return a http stream of bytes
pub struct RawEventClient(pub Receiver<Bytes>);

impl Stream for RawEventClient {
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
  clients: Vec<Sender<Bytes>>,
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
      IoError::interupted("RawEmitterMutex", err.to_string().as_str())
    })?;
    for client in &inner.clients {
      if client.try_send(Bytes::from("")).is_err() {
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
  pub fn emit(&self, e: &Event) -> IoResult<()> {
    let inner = self.inner.lock().map_err(|err| {
      IoError::interupted("RawEmitterMutex", err.to_string().as_str())
    })?;
    for client in &inner.clients {
      match e.try_to_bytes() {
        Ok(msg) => {
          let _ = client.try_send(msg);
        }
        Err(err) => {
          log::error!("raw_emitter: emit {err}");
        }
      }
    }
    Ok(())
  }

  /// Subscribe to events
  pub fn subscribe(&self) -> IoResult<RawEventClient> {
    let (tx, rx) = channel(100);
    self
      .inner
      .lock()
      .map_err(|err| {
        IoError::interupted("RawEmitterMutex", err.to_string().as_str())
      })?
      .clients
      .push(tx);
    Ok(RawEventClient(rx))
  }
}
