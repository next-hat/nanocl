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
      // Acquisition failure grants no ownership of the pending status or lock.
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

#[cfg(test)]
mod tests {
  use nanocl_stubs::system::{
    ObjPsHealthStatusKind, ObjPsStatusKind, ObjPsStatusPartial,
  };

  use crate::{
    models::{DistributedMutexDb, DistributedMutexPartial, ObjPsStatusDb},
    repositories::generic::*,
  };

  use super::*;

  #[ntex::test]
  async fn failed_vm_task_acquisition_preserves_the_active_owner() {
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let pool = &state.inner.pool;
    let key = format!("vm-mutex-test-{}", uuid::Uuid::new_v4());
    for (action, phase, task) in [
      (
        "start",
        ObjPsStatusKind::Starting,
        VmDb::create_start_task(&key, state),
      ),
      (
        "stop",
        ObjPsStatusKind::Stopping,
        VmDb::create_stop_task(&key, state),
      ),
      (
        "update",
        ObjPsStatusKind::Updating,
        VmDb::create_update_task(&key, state),
      ),
      (
        "delete",
        ObjPsStatusKind::Destroying,
        VmDb::create_delete_task(&key, state),
      ),
    ] {
      let status = ObjPsStatusDb::create_from(
        ObjPsStatusPartial {
          key: key.clone(),
          actual: phase,
          wanted: ObjPsStatusKind::Start,
          prev_wanted: ObjPsStatusKind::Stop,
          prev_actual: ObjPsStatusKind::Stop,
          health: ObjPsHealthStatusKind::Unknown,
        },
        pool,
      )
      .await
      .unwrap();
      let owner = DistributedMutexDb::create_from(
        DistributedMutexPartial {
          action: action.to_owned(),
          resource_kind: VmDb::get_kind(),
          resource_key: key.clone(),
          node_key: "another-node".to_owned(),
          acquired_by: "daemon".to_owned(),
        },
        pool,
      )
      .await
      .unwrap();
      let result = task.await;
      let observed = ObjPsStatusDb::read_by_pk(&key, pool).await.unwrap();
      let remaining =
        DistributedMutexDb::get_lock(&VmDb::get_kind(), &key, pool)
          .await
          .unwrap();
      VmDb::delete_mutex(&owner.key, state).await.unwrap();
      ObjPsStatusDb::del_by_pk(&key, pool).await.unwrap();

      assert!(
        result.is_err(),
        "{action} must not acquire another owner's lock"
      );
      assert_eq!(observed.actual, status.actual);
      assert_eq!(observed.prev_actual, status.prev_actual);
      assert_eq!(observed.updated_at, status.updated_at);
      assert_eq!(remaining.key, owner.key);
      assert_eq!(remaining.node_key, owner.node_key);
      assert_eq!(remaining.expires_at, owner.expires_at);
    }
  }
}
