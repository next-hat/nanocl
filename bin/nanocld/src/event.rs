use std::pin::Pin;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Context};

use ntex::{rt, web, http, time};
use ntex::util::Bytes;
use ntex::web::error::BlockingError;
use futures::Stream;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_stubs::system::{Event, ToEvent, EventAction};

use nanocl_error::http::{HttpError, HttpResult};

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
    let self_ptr = Self {
      inner: Arc::new(Mutex::new(EventEmitterInner { clients: vec![] })),
    };
    self_ptr.clone().spawn_check_connection();
    self_ptr
  }

  /// Check if clients are still connected
  fn check_connection(&mut self) -> HttpResult<()> {
    let mut alive_clients = Vec::new();
    let mut inner = self.inner.lock().map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to lock event emitter mutex: {err}"),
    })?;
    for client in &inner.clients {
      let result = client.clone().try_send(Bytes::from(""));
      if let Ok(()) = result {
        alive_clients.push(client.clone());
      }
    }
    inner.clients = alive_clients;
    Ok(())
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(mut self) {
    rt::spawn(async move {
      let task = time::interval(Duration::from_secs(10));
      loop {
        task.tick().await;
        if let Err(err) = self.check_connection() {
          log::error!("{err}");
        }
      }
    });
  }

  /// Send an event to all clients
  pub(crate) async fn emit<T>(
    &self,
    e: T,
    action: EventAction,
  ) -> HttpResult<()>
  where
    T: ToEvent,
  {
    let self_ptr = self.clone();
    let inner = web::block(move || {
      let inner = self_ptr
        .inner
        .lock()
        .map_err(|err| HttpError {
          status: http::StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Unable to lock event emitter mutex: {err}"),
        })?
        .clone();
      Ok::<_, HttpError>(inner)
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
    let ev = e.to_event(action.clone());
    log::debug!(
      "Emitting {} {} to {} client(s)",
      ev.kind,
      ev.action,
      inner.clients.len()
    );
    inner
      .clients
      .into_iter()
      .map(|client| {
        let ev = ev.clone();
        async move {
          let msg = ev.to_bytes()?;
          let _ = client.send(msg).await;
          Ok::<(), HttpError>(())
        }
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await;
    log::debug!("Emitted");
    Ok(())
  }

  /// Call emit in the background
  pub(crate) fn spawn_emit<T>(&self, e: &T, action: EventAction)
  where
    T: ToEvent + Clone + 'static,
  {
    let self_ptr = self.clone();
    let e = e.clone();
    rt::spawn(async move {
      if let Err(err) = self_ptr.emit(e, action).await {
        log::error!("{err}");
      }
    });
  }

  /// Subscribe to events
  pub(crate) async fn subscribe(&self) -> HttpResult<Client> {
    let self_ptr = self.clone();
    let (tx, rx) = channel(100);
    web::block(move || {
      self_ptr
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
  use nanocl_stubs::cargo::Cargo;
  use nanocl_stubs::secret::Secret;
  use nanocl_stubs::resource::Resource;

  pub async fn send_and_parse_events<T>(
    client: &mut Client,
    event_emitter: &EventEmitter,
    events: Vec<(&T, EventAction)>,
  ) where
    T: ToEvent + Clone,
  {
    for event in events {
      event_emitter
        .emit(event.0.clone(), event.1)
        .await
        .expect("Emit event");
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
    let cargo = Cargo::default();
    let vm = Vm::default();
    let resource = Resource::default();
    let secret = Secret::default();
    // test with cargo
    send_and_parse_events(
      &mut client,
      &event_emitter,
      vec![
        (&cargo, EventAction::Created),
        (&cargo, EventAction::Started),
        (&cargo, EventAction::Stopped),
        (&cargo, EventAction::Patched),
        (&cargo, EventAction::Deleted),
      ],
    )
    .await;
    // test with vm
    send_and_parse_events(
      &mut client,
      &event_emitter,
      vec![
        (&vm, EventAction::Created),
        (&vm, EventAction::Started),
        (&vm, EventAction::Stopped),
        (&vm, EventAction::Patched),
        (&vm, EventAction::Deleted),
      ],
    )
    .await;
    // test with resource
    send_and_parse_events(
      &mut client,
      &event_emitter,
      vec![
        (&resource, EventAction::Created),
        (&resource, EventAction::Patched),
        (&resource, EventAction::Deleted),
      ],
    )
    .await;
    // test with secret
    send_and_parse_events(
      &mut client,
      &event_emitter,
      vec![
        (&secret, EventAction::Created),
        (&secret, EventAction::Patched),
        (&secret, EventAction::Deleted),
      ],
    )
    .await;
  }
}
