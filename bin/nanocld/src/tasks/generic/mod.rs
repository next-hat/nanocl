use std::pin::Pin;

use futures_util::Future;

use nanocl_error::io::IoError;

use crate::models::SystemState;

pub type ObjTaskFuture = Pin<Box<dyn Future<Output = Result<(), IoError>>>>;

pub trait ObjTask {}

pub trait ObjTaskStart {
  /// Create a task (future) that will be run when a process object (job, cargo, vm) is starting
  /// This task run on his own event loop hosted by the SystemState
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskDelete {
  /// Create a task (future) that will be run when a process object (job, cargo, vm) is deleting
  /// This task run on his own event loop hosted by the SystemState
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskUpdate {
  /// Create a task (future) that will be run when a process object (job, cargo, vm) is updating
  /// This task run on his own event loop hosted by the SystemState
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskStop {
  /// Create a task (future) that will be run when a process object (job, cargo, vm) is stopping
  /// This task run on his own event loop hosted by the SystemState
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}
