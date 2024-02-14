use std::pin::Pin;

use futures_util::Future;

use nanocl_error::io::IoError;

use crate::models::SystemState;

pub type ObjTaskFuture = Pin<Box<dyn Future<Output = Result<(), IoError>>>>;

pub trait ObjTaskStart {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskDelete {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskUpdate {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}

pub trait ObjTaskStop {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture;
}
