use std::{
  sync::{Arc, Mutex},
  collections::HashMap,
};

use ntex::{rt, web};
use futures_util::Future;

use nanocl_error::io::{IoResult, IoError};

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
}

#[derive(Clone, Default)]
pub struct TaskManager {
  pub tasks: Arc<Mutex<HashMap<String, ObjTask>>>,
}

impl TaskManager {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn add_task(&self, key: &str, task: ObjTask) -> IoResult<()> {
    let key = key.to_owned();
    let tasks = Arc::clone(&self.tasks);
    web::block(move || {
      let mut tasks = tasks.lock()?;
      log::debug!("Adding task: {key} {}", task.kind);
      tasks.insert(key.clone(), task.clone());
      Ok::<_, IoError>(())
    })
    .await?;
    Ok(())
  }

  pub async fn remove_task(&self, key: &str) -> IoResult<()> {
    let key = key.to_owned();
    let tasks = Arc::clone(&self.tasks);
    web::block(move || {
      let mut tasks = tasks.lock().map_err(|err| {
        IoError::interrupted("Task", err.to_string().as_str())
      })?;
      let task = tasks.get(&key);
      if let Some(task) = task {
        log::debug!("Removing task: {key} {}", task.kind);
        task.fut.lock()?.abort();
      }
      tasks.remove(&key);
      Ok::<_, IoError>(())
    })
    .await?;
    Ok(())
  }

  pub async fn get_task(&self, key: &str) -> Option<ObjTask> {
    let key = key.to_owned();
    let tasks = Arc::clone(&self.tasks);
    let res = web::block(move || {
      let tasks = tasks.lock().map_err(|err| {
        IoError::interrupted("Task", err.to_string().as_str())
      })?;
      Ok::<_, IoError>(tasks.get(&key).cloned())
    })
    .await;
    match res {
      Ok(res) => res,
      Err(err) => {
        log::error!("Failed to get task: {}", err);
        None
      }
    }
  }
}
