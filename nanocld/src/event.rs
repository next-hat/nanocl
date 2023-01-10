use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

use ntex::rt;
use ntex::util::HashMap;
use ntex::web;
use ntex::util::Bytes;
use ntex::channel::mpsc::Sender;
use ntex::channel::mpsc::Receiver;
use ntex::channel::mpsc::channel;
use futures::channel::mpsc;
use futures::{stream, StreamExt, SinkExt};

use nanocl_models::cargo::Cargo;
use serde::{Serialize, Deserialize};

use crate::error::HttpResponseError;

#[derive(Debug, Serialize, Deserialize)]
pub enum Event {
  CargoCreated(Cargo),
}

#[derive(Clone)]
pub struct EventEmitter {
  clients: Arc<Mutex<HashMap<usize, mpsc::UnboundedSender<Bytes>>>>,
  sender: mpsc::UnboundedSender<Event>,
}

impl EventEmitter {
  fn lock_mutex<T>(mutex: &'_ Arc<Mutex<T>>) -> Option<MutexGuard<'_, T>> {
    match mutex.lock() {
      Err(err) => {
        eprintln!("Unable to lock clients mutex {:#?}", err);
        None
      }
      Ok(guard) => Some(guard),
    }
  }

  async fn handle_event(&self, e: Event) {
    let clients = match Self::lock_mutex(&self.clients) {
      None => HashMap::default(),
      Some(clients) => clients.to_owned(),
    };
    log::debug!("Sending events {:#?} to clients {:#?}", &e, &clients);
    let mut data = serde_json::to_vec(&e).unwrap();
    data.push(b'\n');
    let bytes = Bytes::from(data);
    let mut stream = stream::iter(clients);
    while let Some((id, mut client)) = stream.next().await {
      if let Err(_err) = client.send(bytes.to_owned()).await {
        self.remove_client(id);
      }
    }
  }

  fn remove_client(&self, id: usize) {
    if let Some(mut c) = Self::lock_mutex(&self.clients) {
      c.remove(&id);
    }
  }

  fn add_client(&self, client: mpsc::UnboundedSender<Bytes>) {
    if let Some(mut clients) = Self::lock_mutex(&self.clients) {
      let id = clients.len() + 1;
      clients.insert(id, client);
    }
  }

  fn pipe_stream(
    mut source: mpsc::UnboundedReceiver<Bytes>,
    dest: Sender<Result<Bytes, web::error::Error>>,
  ) {
    rt::spawn(async move {
      while let Some(bytes) = source.next().await {
        if let Err(err) = dest.send(Ok::<_, web::error::Error>(bytes)) {
          eprintln!("Error while piping stream : {:} closing.", err);
          source.close();
          dest.close();
          break;
        }
      }
    });
  }

  fn r#loop(&self, mut receiver: mpsc::UnboundedReceiver<Event>) {
    // Handle events in a background thread
    let this = self.to_owned();
    rt::Arbiter::new().exec_fn(|| {
      rt::spawn(async move {
        // Start the event loop
        while let Some(event) = receiver.next().await {
          this.handle_event(event).await;
        }
        // If the event loop stop, we stop the current thread
        rt::Arbiter::current().stop();
      });
      rt::spawn(async move {});
    });
  }

  pub fn new() -> EventEmitter {
    let (tx, rx) = mpsc::unbounded::<Event>();
    let event_emitter = EventEmitter {
      sender: tx,
      clients: Arc::new(Mutex::new(HashMap::default())),
    };
    event_emitter.r#loop(rx);
    event_emitter
  }

  pub async fn send(&self, e: Event) -> Result<(), HttpResponseError> {
    let mut sender = self.sender.to_owned();
    rt::spawn(async move {
      let _ = sender.send(e).await;
    });
    Ok(())
  }

  pub fn subscribe(&self) -> Receiver<Result<Bytes, web::error::Error>> {
    let (event_sender, event_receiver) = mpsc::unbounded::<Bytes>();
    let (client_sender, client_receiver) =
      channel::<Result<Bytes, web::error::Error>>();
    Self::pipe_stream(event_receiver, client_sender);
    self.add_client(event_sender);
    client_receiver
  }
}

/*
#[derive(Clone)]
pub struct EventEmitter {
    // ... existing fields ...
    timeout: Duration,
    clients: Arc<Mutex<HashMap<usize, mpsc::UnboundedSender<Bytes>>>>,
    sender: mpsc::UnboundedSender<Event>,
    client_status: Arc<Mutex<HashMap<usize, Instant>>>,
    task_running: Arc<AtomicBool>,
}

impl EventEmitter {
    // ... existing methods ...
    async fn check_clients_timeout(&self) {
        let clients = match Self::lock_mutex(&self.clients) {
            None => HashMap::default(),
            Some(clients) => clients.to_owned(),
        };
        let client_status = match Self::lock_mutex(&self.client_status) {
            None => HashMap::default(),
            Some(client_status) => client_status.to_owned(),
        };
        let now = Instant::now();
        for (id, instant) in client_status {
            if now.duration_since(instant) >= self.timeout {
                clients.remove(&id);
                client_status.remove(&id);
            }
        }

fn add_client(&self, client: mpsc::UnboundedSender<Bytes>) {
  if let Some(mut clients) = Self::lock_mutex(&self.clients) {
    let id = clients.len() + 1;
    clients.insert(id, client);
  if let Some(mut client_status) = Self::lock_mutex(&self.client_status) {
    client_status.insert(id, Instant::now());
    }
  }
}

fn remove_client(&self, id: usize) {
if let Some(mut clients) = Self::lock_mutex(&self.clients) {
clients.remove(&id);
}
  if let Some(mut client_status) = Self::lock_mutex(&self.client_status) {
    client_status.remove(&id);
  }
}

pub fn new() -> EventEmitter {
  let (tx, rx) = mpsc::unbounded::<Event>();
  let clients = Arc::new(Mutex::new(HashMap::new()));
  let client_status = Arc::new(Mutex::new(HashMap::new()));
  let task_running = Arc::new(AtomicBool::new(false));
  let event_emitter = EventEmitter {
  // ...
    clients,
    sender: tx,
  client_status,
  task_running,
};
  rt::spawn(async move {
      loop {
        delay_for(Duration::from_secs(5)).await;
        event_emitter.check_clients_timeout().await;
    }
});
    event_emitter
  }
}
 */
