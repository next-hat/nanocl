use nanocl_error::io::IoResult;
use nanocl_stubs::{
  process::ProcessKind,
  system::{EventActorKind, ObjPsStatusKind},
};

use crate::{
  models::{DistributedMutexDb, ObjPsStatusDb, SystemState, VmDb},
  repositories::generic::*,
  utils,
};

use super::generic::*;

fn failed_setup_owns_transition(action: &str, actual: &str) -> bool {
  matches!(
    (action, actual),
    ("start", "starting")
      | ("stop", "stopping")
      | ("update", "updating")
      | ("delete", "destroying")
  )
}

async fn create_vm_mutex(
  key: &str,
  action: &str,
  state: &SystemState,
) -> IoResult<DistributedMutexDb> {
  match VmDb::create_mutex(key, action, state).await {
    Ok(mutex) => Ok(mutex),
    Err(error) => {
      // Runtime helpers have not been entered yet. Repair only this pending
      // transition under the same local guard; never delete an unowned mutex.
      let _guard = utils::container::vm::lifecycle_lock(key).lock_owned().await;
      let repair = async {
        let status = ObjPsStatusDb::read_by_pk(key, &state.inner.pool).await?;
        if failed_setup_owns_transition(action, &status.actual) {
          ObjPsStatusDb::update_actual_status(
            key,
            &ObjPsStatusKind::Fail,
            &state.inner.pool,
          )
          .await?;
        }
        Ok::<_, nanocl_error::io::IoError>(())
      }
      .await;
      if let Err(repair_error) = repair {
        log::error!(
          "Unable to repair VM {key} status after {error}: {repair_error}"
        );
      }
      Err(error)
    }
  }
}

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
      let mutex = create_vm_mutex(&key, "start", &state).await?;
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
      let mutex = create_vm_mutex(&key, "stop", &state).await?;
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
      let mutex = create_vm_mutex(&key, "delete", &state).await?;
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
      let mutex = create_vm_mutex(&key, "update", &state).await?;
      let result = utils::container::vm::update(&key, &state).await;
      let cleanup = VmDb::delete_mutex(&mutex.key, &state).await;
      result.and(cleanup)
    })
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn failed_vm_task_setup_only_repairs_its_pending_transition() {
    for (action, phase) in [
      ("start", "starting"),
      ("stop", "stopping"),
      ("update", "updating"),
      ("delete", "destroying"),
    ] {
      assert!(super::failed_setup_owns_transition(action, phase));
      for observed in ["start", "stop", "fail", "destroy", "create"] {
        assert!(!super::failed_setup_owns_transition(action, observed));
      }
    }
    assert!(!super::failed_setup_owns_transition("start", "updating"));
    assert!(!super::failed_setup_owns_transition("update", "destroying"));
  }
}
