use nanocl_error::io::IoError;
use nanocl_stubs::process::ProcessKind;

use crate::{
  models::{SystemState, VmDb},
  utils,
};

use super::generic::*;

impl ObjTaskStart for VmDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::vm::start(&key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for VmDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::process::stop_instances(&key, &ProcessKind::Vm, &state)
        .await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for VmDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::vm::delete(&key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskUpdate for VmDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::vm::update(&key, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}
