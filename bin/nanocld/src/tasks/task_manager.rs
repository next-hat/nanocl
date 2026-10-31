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
    // Registration must precede execution, and completion must remove only
    // this handle. A delayed waiter/callback must not cancel its successor.
    let (registered, identity) = tokio::sync::oneshot::channel();
    let new_task = ObjTask::new(kind.clone(), async move {
      let identity = identity.await.map_err(|_| {
        IoError::interrupted("Task manager", "Task registration failed")
      })?;
      let result = task.await;
      {
        let mut tasks = tasks.lock().await;
        if tasks.get(&key_ptr).is_some_and(|current| {
          std::sync::Weak::ptr_eq(&Arc::downgrade(&current.fut), &identity)
        }) {
          tasks.remove(&key_ptr);
        }
      }
      if let Err(err) = result {
        log::error!("Task failed: {kind} {key_ptr} {}", err);
        if let Err(err) = on_error(err.clone()).await {
          log::error!("on_error failed: {kind} {key_ptr} {}", err);
          return Err(err);
        }
        return Err(err);
      };
      log::debug!("Task completed: {kind} {key_ptr}");
      Ok::<_, IoError>(())
    });
    let identity = Arc::downgrade(&new_task.fut);
    self.tasks.lock().await.insert(key, new_task);
    let _ = registered.send(identity);
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
      self.wait_registered_task(key, task).await;
    }
  }

  async fn wait_registered_task(&self, key: &str, task: ObjTask) {
    task.wait().await;
    let mut tasks = self.tasks.lock().await;
    if tasks
      .get(key)
      .is_some_and(|current| Arc::ptr_eq(&current.fut, &task.fut))
    {
      log::debug!("Task finished: {key} {} removing it", task.kind);
      tasks.remove(key);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[ntex::test]
  async fn delayed_task_waiter_preserves_the_registered_successor() {
    let manager = TaskManager::new();
    manager
      .add_task(
        "Vm@test",
        NativeEventAction::Starting,
        Box::pin(async { Ok(()) }),
        |_| async { Ok(()) },
      )
      .await;
    let previous = manager.get_task("Vm@test").await.unwrap();
    previous.wait().await;

    let (complete, blocked) = tokio::sync::oneshot::channel();
    manager
      .add_task(
        "Vm@test",
        NativeEventAction::Updating,
        Box::pin(async move {
          let _ = blocked.await;
          Ok(())
        }),
        |_| async { Ok(()) },
      )
      .await;
    let successor = manager.get_task("Vm@test").await.unwrap();
    // The completion event captured the previous handle before the new
    // request registered its task, then resumed its cleanup afterward.
    manager.wait_registered_task("Vm@test", previous).await;
    let remaining = manager.get_task("Vm@test").await.unwrap();
    assert!(Arc::ptr_eq(&successor.fut, &remaining.fut));
    assert!(!successor.fut.lock().await.as_ref().unwrap().is_finished());
    complete.send(()).unwrap();
    manager.wait_task("Vm@test").await;
  }

  #[ntex::test]
  async fn delayed_task_failure_preserves_the_registered_successor() {
    let manager = TaskManager::new();
    let (failed, failure_started) = tokio::sync::oneshot::channel();
    let (release, callback_blocked) = tokio::sync::oneshot::channel();
    manager
      .add_task(
        "Vm@test",
        NativeEventAction::Starting,
        Box::pin(async {
          Err(IoError::other("test", "injected setup failure"))
        }),
        move |_| async move {
          failed.send(()).unwrap();
          let _ = callback_blocked.await;
          Ok(())
        },
      )
      .await;
    let previous = manager.get_task("Vm@test").await.unwrap();
    failure_started.await.unwrap();
    let (complete, blocked) = tokio::sync::oneshot::channel();
    manager
      .add_task(
        "Vm@test",
        NativeEventAction::Updating,
        Box::pin(async move {
          let _ = blocked.await;
          Ok(())
        }),
        |_| async { Ok(()) },
      )
      .await;
    let successor = manager.get_task("Vm@test").await.unwrap();
    release.send(()).unwrap();
    manager.wait_registered_task("Vm@test", previous).await;
    let remaining = manager.get_task("Vm@test").await.unwrap();
    assert!(Arc::ptr_eq(&successor.fut, &remaining.fut));
    complete.send(()).unwrap();
    manager.wait_task("Vm@test").await;
  }
}
