use nanocl_error::io::IoError;
use nanocl_stubs::system::NativeEventAction;

use crate::models::{ObjTask, TaskManager};

use super::generic::ObjTaskFuture;

use std::{sync::Arc, time::Duration};

use futures_util::Future;
use ntex::{rt, time};

use nanocl_error::io::IoResult;

impl ObjTask {
  pub fn new<F>(kind: NativeEventAction, task: F) -> Self
  where
    F: Future<Output = IoResult<()>> + 'static,
  {
    let fut = Arc::new(futures_util::lock::Mutex::new(Some(rt::spawn(task))));
    Self { kind, fut }
  }

  pub async fn wait(&self) {
    loop {
      let is_finished = {
        let fut = self.fut.lock().await;
        fut.as_ref().is_none_or(|f| f.is_finished())
      };
      if is_finished {
        log::debug!("Task finished: {}", self.kind);
        break;
      }
      time::sleep(Duration::from_micros(10)).await;
    }
  }
}

impl TaskManager {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn add_task<EC, F>(
    &self,
    key: &str,
    kind: NativeEventAction,
    task: ObjTaskFuture,
    on_error: EC,
  ) where
    EC: FnOnce(IoError) -> F + 'static,
    F: Future<Output = IoResult<()>> + 'static,
  {
    let key = key.to_owned();
    let key_ptr = key.clone();
    let tasks = self.tasks.clone();
    log::debug!("Creating task: {key} {}", kind);
    let new_task = ObjTask::new(kind.clone(), async move {
      if let Err(err) = task.await {
        log::error!("Task failed: {kind} {key_ptr} {}", err);
        tasks.lock().await.remove(&key_ptr);
        if let Err(err) = on_error(err.clone()).await {
          log::error!("on_error failed: {kind} {key_ptr} {}", err);
          return Err(err);
        }
        return Err(err);
      };
      log::debug!("Task completed: {kind} {key_ptr}");
      tasks.lock().await.remove(&key_ptr);
      Ok::<_, IoError>(())
    });
    self.tasks.lock().await.insert(key, new_task);
  }

  pub async fn remove_task(&self, key: &str) {
    let mut tasks = self.tasks.lock().await;
    if let Some(task) = tasks.remove(key) {
      let kind = task.kind.clone();
      let handle = {
        let mut fut = task.fut.lock().await;
        fut.take()
      };
      if let Some(handle) = handle
        && !handle.is_finished()
      {
        log::debug!("Cancelling task: {key} {kind}");
        handle.cancel();
      }
      log::debug!("Removing task: {key} {kind}");
    }
  }

  pub async fn get_task(&self, key: &str) -> Option<ObjTask> {
    let tasks = self.tasks.lock().await;
    tasks.get(key).cloned()
  }

  pub async fn wait_task(&self, key: &str) {
    if let Some(task) = self.get_task(key).await {
      task.wait().await;
      log::debug!("Task finished: {key} {} removing it", task.kind);
      self.remove_task(key).await;
    }
  }
}
