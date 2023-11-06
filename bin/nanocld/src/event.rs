use std::pin::Pin;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Context};

use ntex::{rt, web, http, time};
use ntex::util::Bytes;
use ntex::web::error::BlockingError;
use futures::Stream;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_stubs::system::Event;

use nanocl_error::http::HttpError;

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
        let task = time::interval(Duration::from_secs(10));
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
  use nanocl_stubs::resource::Resource;
  use nanocl_stubs::secret::Secret;

  use super::*;

  use nanocl_stubs::vm::Vm;
  use nanocl_stubs::cargo::CargoInspect;

  pub async fn send_and_parse_events(
    client: &mut Client,
    event_emitter: &EventEmitter,
    events: Vec<Event>,
  ) {
    for event in events {
      event_emitter
        .emit(event.clone())
        .await
        .unwrap_or_else(|err| panic!("Event emit failed {event:#?} {err}"));
      let event = client
        .next()
        .await
        .unwrap_or_else(|| panic!("No event received"))
        .unwrap_or_else(|err| panic!("Event error {err}"));
      let _ = serde_json::from_slice::<Event>(&event)
        .unwrap_or_else(|err| panic!("Parse event error {err}"));
    }
  }

  #[ntex::test]
  async fn basic() {
    let event_emitter = EventEmitter::new();
    let mut client = event_emitter.subscribe().await.unwrap();
    let cargo = CargoInspect::default();
    let vm = Vm::default();
    let resource = Resource::default();
    let secret = Secret::default();

    send_and_parse_events(
      &mut client,
      &event_emitter,
      vec![
        Event::NamespaceCreated("test".to_owned()),
        Event::CargoCreated(Box::new(cargo.clone())),
        Event::CargoDeleted(Box::new(cargo.clone())),
        Event::CargoStarted(Box::new(cargo.clone())),
        Event::CargoStopped(Box::new(cargo.clone())),
        Event::CargoPatched(Box::new(cargo.clone())),
        Event::VmCreated(Box::new(vm.clone())),
        Event::VmDeleted(Box::new(vm.clone())),
        Event::VmPatched(Box::new(vm.clone())),
        Event::VmRunned(Box::new(vm.clone())),
        Event::VmStopped(Box::new(vm.clone())),
        Event::ResourceCreated(Box::new(resource.clone())),
        Event::ResourceDeleted(Box::new(resource.clone())),
        Event::ResourcePatched(Box::new(resource.clone())),
        Event::SecretCreated(Box::new(secret.clone())),
        Event::SecretDeleted(Box::new(secret.clone())),
        Event::SecretPatched(Box::new(secret.clone())),
      ],
    )
    .await;
  }
}
