use std::{
  pin::Pin,
  time::Duration,
  sync::{Arc, Mutex},
  task::{Poll, Context},
};

use ntex::{rt, web, time, util::Bytes, web::error::BlockingError};
use futures::Stream;
use futures_util::{StreamExt, stream::FuturesUnordered};
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::system::Event;

/// Stream: Wrap Receiver in our own type, with correct error type
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

impl Client {
  pub async fn recv(&mut self) -> Option<Bytes> {
    self.0.recv().await
  }
}

trait TryToBytes {
  type Error;

  fn try_to_bytes(&self) -> Result<Bytes, Self::Error>;
}

impl TryToBytes for Event {
  type Error = HttpError;

  fn try_to_bytes(&self) -> Result<Bytes, Self::Error> {
    let mut data = serde_json::to_vec(&self).map_err(|err| {
      HttpError::internal_server_error(format!(
        "Unable to serialize event: {err}"
      ))
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
    let mut inner = self.inner.lock().map_err(|err| {
      HttpError::internal_server_error(format!(
        "Unable to lock event emitter mutex: {err}"
      ))
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
    rt::Arbiter::new().exec_fn(|| {
      rt::spawn(async move {
        let task = time::interval(Duration::from_secs(10));
        loop {
          task.tick().await;
          if let Err(err) = self.check_connection() {
            log::error!("{err}");
          }
        }
      });
    });
  }

  /// Send an event to all clients
  pub(crate) async fn emit(&self, e: Event) -> HttpResult<()> {
    let self_ptr = self.clone();
    let inner = web::block(move || {
      let inner = self_ptr
        .inner
        .lock()
        .map_err(|err| {
          HttpError::internal_server_error(format!(
            "Unable to lock event emitter mutex: {err}"
          ))
        })?
        .clone();
      Ok::<_, HttpError>(inner)
    })
    .await
    .map_err(|err| match err {
      BlockingError::Error(err) => err,
      BlockingError::Canceled => HttpError::internal_server_error(
        "Unable to subscribe to metrics server future got cancelled",
      ),
    })?;
    log::debug!(
      "event_emitter::emit: {}:{} to {} client(s)",
      e.kind,
      e.action,
      inner.clients.len()
    );
    inner
      .clients
      .into_iter()
      .map(|client| {
        let e = e.clone();
        async move {
          let msg = e.try_to_bytes()?;
          let _ = client.send(msg).await;
          Ok::<(), HttpError>(())
        }
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await;
    log::debug!("event_emitter::emit: done");
    Ok(())
  }

  pub fn spawn_emit_event(&self, e: Event) {
    let self_ptr = self.clone();
    rt::spawn(async move {
      if let Err(err) = self_ptr.emit(e).await {
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
        .map_err(|err| {
          HttpError::internal_server_error(format!(
            "Unable to lock event emitter mutex: {err}"
          ))
        })?
        .clients
        .push(tx);
      Ok::<(), HttpError>(())
    })
    .await
    .map_err(|err| match err {
      BlockingError::Error(err) => err,
      BlockingError::Canceled => HttpError::internal_server_error(
        "Unable to subscribe to metrics server future got cancelled",
      ),
    })?;
    Ok(Client(rx))
  }
}
