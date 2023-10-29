use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;
use std::task::Context;
use std::time::Duration;

use ntex::{rt, web, http};
use ntex::util::Bytes;
use ntex::time::interval;
use ntex::web::error::BlockingError;
use futures::Stream;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_stubs::system::Event;

use nanocl_utils::http_error::HttpError;

/// ## Client
/// Stream: Wrap Receiver in our own type, with correct error type
///
pub struct Client(pub Receiver<Bytes>);

impl Stream for Client {
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

trait ToBytes {
  type Error;

  fn to_bytes(&self) -> Result<Bytes, Self::Error>;
}

impl ToBytes for Event {
  type Error = HttpError;

  fn to_bytes(&self) -> Result<Bytes, Self::Error> {
    let mut data = serde_json::to_vec(&self).map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to serialize event: {err}"),
    })?;
    data.push(b'\n');
    Ok(Bytes::from(data))
  }
}

#[derive(Clone, Default)]
pub struct EventEmitter {
  inner: Arc<Mutex<EventEmitterInner>>,
}

#[derive(Clone, Default)]
pub struct EventEmitterInner {
  clients: Vec<Sender<Bytes>>,
}

impl EventEmitter {
  pub fn new() -> Self {
    let this = Self {
      inner: Arc::new(Mutex::new(EventEmitterInner { clients: vec![] })),
    };
    this.clone().spawn_check_connection();
    this
  }

  /// Check if clients are still connected
  fn check_connection(&mut self) -> Result<(), HttpError> {
    let mut alive_clients = Vec::new();
    let clients = self
      .inner
      .lock()
      .map_err(|err| HttpError {
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to lock event emitter mutex: {err}"),
      })?
      .clients
      .clone();
    for client in clients {
      let result = client.clone().try_send(Bytes::from(""));
      if let Ok(()) = result {
        alive_clients.push(client.clone());
      }
    }
    self
      .inner
      .lock()
      .map_err(|err| HttpError {
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to lock event emitter mutex: {err}"),
      })?
      .clients = alive_clients;
    Ok(())
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(mut self) {
    rt::spawn(async move {
      loop {
        let task = interval(Duration::from_secs(10));
        task.tick().await;
        if let Err(err) = self.check_connection() {
          log::error!("{err}");
        }
      }
    });
  }

  /// Send an event to all clients
  pub async fn emit(&self, ev: Event) -> Result<(), HttpError> {
    let this = self.clone();
    rt::spawn(async move {
      let clients = this
        .inner
        .lock()
        .map_err(|err| HttpError {
          status: http::StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Unable to lock event emitter mutex: {err}"),
        })?
        .clients
        .clone();
      for client in clients {
        let msg = ev.to_bytes()?;
        let _ = client.send(msg.clone()).await;
      }
      Ok::<(), HttpError>(())
    })
    .await
    .map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to spawn task to emit message: {err}"),
    })??;
    Ok(())
  }

  /// Subscribe to events
  pub async fn subscribe(&self) -> Result<Client, HttpError> {
    let this = self.clone();
    let (tx, rx) = channel(100);
    web::block(move || {
      this
        .inner
        .lock()
        .map_err(|err| HttpError {
          status: http::StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Unable to lock event emitter mutex: {err}"),
        })?
        .clients
        .push(tx);
      Ok::<(), HttpError>(())
    })
    .await
    .map_err(|err| match err {
      BlockingError::Error(err) => err,
      BlockingError::Canceled => HttpError {
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
        msg: "Unable to subscribe to metrics server furture got cancelled"
          .into(),
      },
    })?;
    Ok(Client(rx))
  }
}

#[cfg(test)]
mod tests {
  use futures::StreamExt;

  use super::*;

  use nanocl_stubs::vm::Vm;
  use nanocl_stubs::cargo::CargoInspect;

  #[ntex::test]
  async fn basic() {
    // Create the event emitter
    let event_emitter = EventEmitter::new();
    // Create a client
    let mut client = event_emitter.subscribe().await.unwrap();
    // Send namespace created event
    event_emitter
      .emit(Event::NamespaceCreated("test".to_string()))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send cargo created event
    let cargo = CargoInspect::default();
    event_emitter
      .emit(Event::CargoCreated(Box::new(cargo.to_owned())))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send cargo deleted event
    event_emitter
      .emit(Event::CargoDeleted(Box::new(cargo)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send cargo started event
    let cargo = CargoInspect::default();
    event_emitter
      .emit(Event::CargoStarted(Box::new(cargo)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send cargo stopped event
    let cargo = CargoInspect::default();
    event_emitter
      .emit(Event::CargoStopped(Box::new(cargo)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send cargo patched event
    let cargo = CargoInspect::default();
    event_emitter
      .emit(Event::CargoPatched(Box::new(cargo)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send vm created event
    let vm = Vm::default();
    event_emitter
      .emit(Event::VmCreated(Box::new(vm)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send vm deleted event
    let vm = Vm::default();
    event_emitter
      .emit(Event::VmDeleted(Box::new(vm)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send vm patched event
    let vm = Vm::default();
    event_emitter
      .emit(Event::VmPatched(Box::new(vm)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send vm runned event
    let vm = Vm::default();
    event_emitter
      .emit(Event::VmRunned(Box::new(vm)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
    // Send vm stopped event
    let vm = Vm::default();
    event_emitter
      .emit(Event::VmStopped(Box::new(vm)))
      .await
      .unwrap();
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();
  }
}
