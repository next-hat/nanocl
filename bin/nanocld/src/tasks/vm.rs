use nanocl_stubs::{process::ProcessKind, system::EventActorKind};

use crate::{
  models::{SystemState, VmDb},
  utils,
};

use super::generic::*;

impl ObjTask for VmDb {
  fn get_kind() -> String {
    EventActorKind::Vm.to_string()
  }
}

impl ObjTaskStart for VmDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = VmDb::create_mutex(&key, "start", &state).await?;
      let result = utils::container::vm::start(&key, &state).await;
      let cleanup = VmDb::delete_mutex(&mutex.key, &state).await;
      result.and(cleanup)
    })
  }
}

impl ObjTaskStop for VmDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = VmDb::create_mutex(&key, "stop", &state).await?;
      let result = utils::container::process::stop_instances(
        &key,
        &ProcessKind::Vm,
        &state,
      )
      .await;
      let cleanup = VmDb::delete_mutex(&mutex.key, &state).await;
      result.and(cleanup)
    })
  }
}

impl ObjTaskDelete for VmDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = VmDb::create_mutex(&key, "delete", &state).await?;
      let result = utils::container::vm::delete(&key, &state).await;
      let cleanup = VmDb::delete_mutex(&mutex.key, &state).await;
      result.and(cleanup)
    })
  }
}

impl ObjTaskUpdate for VmDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let mutex = VmDb::create_mutex(&key, "update", &state).await?;
      let result = utils::container::vm::update(&key, &state).await;
      let cleanup = VmDb::delete_mutex(&mutex.key, &state).await;
      result.and(cleanup)
    })
  }
}
