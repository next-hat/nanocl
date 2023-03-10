use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Poll;
use std::task::Context;
use std::time::Duration;

use ntex::rt;
use ntex::web::Error;
use ntex::util::Bytes;
use ntex::time::interval;
use futures::Stream;
use futures::{stream, StreamExt};
use tokio::sync::mpsc::{Receiver, Sender, channel};

use nanocl_stubs::system::Event;

// Wrap Receiver in our own type, with correct error type
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

#[derive(Clone, Default)]
pub struct EventEmitter {
  clients: Vec<Sender<Bytes>>,
}

pub type EventEmitterPtr = Arc<Mutex<EventEmitter>>;

impl EventEmitter {
  /// Convert an event to a string and send it to all clients
  async fn handle_event(&mut self, e: Event) {
    let mut data = serde_json::to_vec(&e).unwrap();
    data.push(b'\n');
    let bytes = Bytes::from(data);
    let mut stream = stream::iter(self.clients.to_owned());
    while let Some(client) = stream.next().await {
      client.send(bytes.to_owned()).await.unwrap_or(());
    }
  }

  /// Add a client to the list of clients
  fn add_client(&mut self, client: Sender<Bytes>) {
    self.clients.push(client);
  }

  /// Check if clients are still connected
  fn check_connection(&mut self) {
    let mut alive_clients = Vec::new();
    for client in &self.clients {
      let result = client.clone().try_send(Bytes::from(""));
      if let Ok(()) = result {
        alive_clients.push(client.clone());
      }
    }
    log::debug!("alive clients : {:#?}", &alive_clients.len());
    self.clients = alive_clients;
  }

  /// Spawn a task that will check if clients are still connected
  fn spawn_check_connection(this: Arc<Mutex<Self>>) {
    rt::spawn(async move {
      loop {
        let task = interval(Duration::from_secs(10));
        task.tick().await;
        this.lock().unwrap().check_connection();
      }
    });
  }

  /// Create a new event emitter
  pub fn new() -> Arc<Mutex<EventEmitter>> {
    let event_emitter = Arc::new(Mutex::new(EventEmitter::default()));

    Self::spawn_check_connection(event_emitter.to_owned());
    event_emitter
  }

  /// Send an event to all clients
  pub fn send(&mut self, e: Event) {
    let mut this = self.clone();
    rt::spawn(async move {
      this.handle_event(e).await;
    });
  }

  /// Subscribe to events
  pub fn subscribe(&mut self) -> Client {
    let (client_sender, client_receiver) = channel::<Bytes>(100);
    self.add_client(client_sender);
    Client(client_receiver)
  }
}

#[cfg(test)]
mod tests {

  use super::*;

  use nanocl_stubs::cargo::CargoInspect;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() -> TestRet {
    // Create the event emitter
    let event_emitter = EventEmitter::new();

    // Create a client
    let mut client = event_emitter.lock().unwrap().subscribe();

    // Send namespace created event
    event_emitter
      .lock()
      .unwrap()
      .send(Event::NamespaceCreated("test".to_string()));

    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    // Send cargo created event
    let cargo = CargoInspect::default();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoCreated(Box::new(cargo.to_owned())));
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    // Send cargo deleted event
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoDeleted(Box::new(cargo)));

    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    // Send cargo started event
    let cargo = CargoInspect::default();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoStarted(Box::new(cargo)));
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    // Send cargo stopped event
    let cargo = CargoInspect::default();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoStopped(Box::new(cargo)));
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    // Send cargo patched event
    let cargo = CargoInspect::default();
    event_emitter
      .lock()
      .unwrap()
      .send(Event::CargoPatched(Box::new(cargo)));
    let event = client.next().await.unwrap().unwrap();
    let _ = serde_json::from_slice::<Event>(&event).unwrap();

    Ok(())
  }
}
