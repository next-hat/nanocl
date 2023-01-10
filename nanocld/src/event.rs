use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use futures::Stream;
use ntex::rt;
use ntex::time::interval;
use ntex::util::Bytes;
use ntex::web::Error;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use futures::{stream, StreamExt};

use nanocl_models::cargo::CargoInspect;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum Event {
  NamespaceCreated(String),
  CargoCreated(Box<CargoInspect>),
  CargoDeleted(String),
  CargoStarted(Box<CargoInspect>),
  CargoStopped(Box<CargoInspect>),
  CargoPatched(Box<CargoInspect>),
}

#[derive(Clone, Default)]
pub struct EventEmitter {
  clients: Vec<Sender<Bytes>>,
}

impl EventEmitter {
  async fn handle_event(&mut self, e: Event) {
    log::debug!("Sending events {:#?} to clients {:#?}", &e, &self.clients);
    let mut data = serde_json::to_vec(&e).unwrap();
    data.push(b'\n');
    let bytes = Bytes::from(data);
    let mut stream = stream::iter(self.clients.to_owned());
    while let Some(client) = stream.next().await {
      client.send(bytes.to_owned()).await.unwrap_or(());
    }
  }

  fn add_client(&mut self, client: Sender<Bytes>) {
    self.clients.push(client);
  }

  fn check_connection(&mut self) {
    let mut new_clients = Vec::new();
    for client in &self.clients {
      let result = client.clone().try_send(Bytes::from(""));
      if let Ok(()) = result {
        new_clients.push(client.clone());
      }
    }
    log::debug!("new clients : {:#?}", &new_clients.len());
    self.clients = new_clients;
  }

  fn spawn_check_connection(this: Arc<Mutex<Self>>) {
    rt::spawn(async move {
      let task = interval(Duration::from_secs(1));
      task.tick().await;
      this.lock().unwrap().check_connection();
      Self::spawn_check_connection(this);
    });
  }

  pub fn new() -> Arc<Mutex<EventEmitter>> {
    let event_emitter = Arc::new(Mutex::new(EventEmitter::default()));

    Self::spawn_check_connection(event_emitter.to_owned());
    event_emitter
  }

  pub fn send(&mut self, e: Event) {
    let mut this = self.to_owned();
    rt::spawn(async move {
      this.handle_event(e).await;
    });
  }

  pub fn subscribe(&mut self) -> Client {
    let (client_sender, client_receiver) = channel::<Bytes>(100);
    self.add_client(client_sender);
    Client(client_receiver)
  }
}

// wrap Receiver in own type, with correct error type
pub struct Client(Receiver<Bytes>);

impl Stream for Client {
  type Item = Result<Bytes, Error>;

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
