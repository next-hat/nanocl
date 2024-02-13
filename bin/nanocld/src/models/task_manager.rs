use std::{sync::Arc, collections::HashMap, time::Duration};

use ntex::{rt, time};
use futures_util::{Future, lock::Mutex};

use nanocl_error::io::IoResult;

use nanocl_stubs::system::NativeEventAction;

#[derive(Clone)]
pub struct ObjTask {
  pub kind: NativeEventAction,
  pub fut: Arc<Mutex<rt::JoinHandle<IoResult<()>>>>,
}

impl ObjTask {
  pub fn new<F>(kind: NativeEventAction, task: F) -> Self
  where
    F: Future<Output = IoResult<()>> + 'static,
  {
    let fut = Arc::new(Mutex::new(rt::spawn(task)));
    Self { kind, fut }
  }

  pub async fn wait(&self) {
    loop {
      let fut = self.fut.lock().await;
      if fut.is_finished() {
        break;
      }
      drop(fut);
      time::sleep(Duration::from_secs(1)).await;
    }
  }
}

#[derive(Clone, Default)]
pub struct TaskManager {
  pub tasks: Arc<Mutex<HashMap<String, ObjTask>>>,
}

impl TaskManager {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn add_task(&self, key: &str, task: ObjTask) {
    let key = key.to_owned();
    let mut tasks = self.tasks.lock().await;
    log::debug!("Adding task: {key} {}", task.kind);
    tasks.insert(key.clone(), task.clone());
  }

  pub async fn remove_task(&self, key: &str) {
    let key = key.to_owned();
    let mut tasks = self.tasks.lock().await;
    let task = tasks.get(&key);
    if let Some(task) = task {
      task.fut.lock().await.abort();
      log::debug!("Removing task: {key} {}", task.kind);
      tasks.remove(&key);
    }
  }

  pub async fn get_task(&self, key: &str) -> Option<ObjTask> {
    let key = key.to_owned();
    let tasks = self.tasks.lock().await;
    tasks.get(&key).cloned()
  }

  pub async fn wait_task(&self, key: &str) {
    if let Some(task) = self.get_task(key).await {
      task.wait().await;
      self.remove_task(key).await;
    }
  }
}
