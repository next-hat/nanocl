use std::pin::Pin;

use futures_util::Future;

use nanocl_error::io::{IoError, IoResult};

use crate::{
  models::{DistributedMutexDb, DistributedMutexPartial, SystemState},
  repositories::generic::*,
};

pub type ObjTaskFuture = Pin<Box<dyn Future<Output = Result<(), IoError>>>>;

pub trait ObjTask {
  fn get_kind() -> String;

  async fn create_mutex(
    key: &str,
    action: &str,
    state: &SystemState,
  ) -> IoResult<DistributedMutexDb> {
    DistributedMutexDb::create_from(
      DistributedMutexPartial {
        action: action.to_owned(),
        resource_kind: Self::get_kind(),
        resource_key: key.to_owned(),
        node_key: state.inner.config.hostname.clone(),
        acquired_by: "daemon".to_owned(),
      },
      &state.inner.pool,
    )
    .await
  }

  async fn delete_mutex(pk: &uuid::Uuid, state: &SystemState) -> IoResult<()> {
    DistributedMutexDb::del_by_pk(pk, &state.inner.pool).await
  }
}

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
