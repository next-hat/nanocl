use std::future::Future;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::{
  process::ProcessKind,
  system::{
    Event, EventActor, EventActorKind, EventKind, EventPartial,
    NativeEventAction,
  },
  vm::Vm,
};

use crate::{
  models::{DistributedMutexDb, EventDb, SystemState, VmDb},
  repositories::generic::*,
  utils, vars,
};

use super::generic::*;

// Acceptance owns both locks before polling the desired/status mutation. The
// detached owner hands them directly to the registered task, so disconnects or
// event delivery failures cannot strand a live pending transition.
pub(crate) async fn accept_task<F>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: F,
) -> HttpResult<Vm>
where
  F: Future<Output = HttpResult<Vm>> + 'static,
{
  let mutex_action = match action {
    NativeEventAction::Starting => "start",
    NativeEventAction::Updating => "update",
    NativeEventAction::Destroying => "delete",
    _ => return Err(HttpError::bad_request("Unsupported VM task action")),
  };
  let acquire_key = key.to_owned();
  let acquire_state = state.clone();
  accept_task_with_acquisition(key, action, state, prepare, async move {
    VmDb::create_mutex(&acquire_key, mutex_action, &acquire_state).await
  })
  .await
}

async fn accept_task_with_acquisition<F, A>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: F,
  acquire: A,
) -> HttpResult<Vm>
where
  F: Future<Output = HttpResult<Vm>> + 'static,
  A: Future<Output = IoResult<DistributedMutexDb>> + 'static,
{
  let guard = utils::container::vm::lifecycle_lock(key)
    .try_lock_owned()
    .await
    .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
  let key = key.to_owned();
  let state = state.clone();
  let (reply, accepted) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let task_key = format!("Vm@{key}");
    // A preceding task may have released its lifecycle guard just before its
    // TaskManager completion bookkeeping. Retire that exact task before reuse.
    state.inner.task_manager.wait_task(&task_key).await;
    let mutex = match acquire.await {
      Ok(mutex) => mutex,
      Err(error) => {
        log::warn!("VM task setup rejected: {key} {action}: {error}");
        let _ = reply.send(Err(error.into()));
        return;
      }
    };
    let vm = match prepare.await {
      Ok(vm) => vm,
      Err(error) => {
        if let Err(cleanup) = VmDb::delete_mutex(&mutex.key, &state).await {
          log::error!(
            "VM acceptance cleanup failed: {key} {}: {cleanup}",
            mutex.key
          );
        }
        let _ = reply.send(Err(error));
        return;
      }
    };
    let revision = vm.spec.key;
    let operation = mutex.key;
    let task_state = state.clone();
    let task_action = action.clone();
    let task_vm = vm.clone();
    let error_state = state.clone();
    let error_action = action.clone();
    let error_actor: EventActor = vm.clone().into();
    let (start, ready) = tokio::sync::oneshot::channel();
    let task = Box::pin(async move {
      let _guard = guard;
      // The task is registered before its acceptance notification, but cannot
      // finish before subscribers have been told it was registered.
      let _ = ready.await;
      let result = async {
        let current = match VmDb::transform_read_by_pk(&key, &task_state.inner.pool).await {
          Ok(current) => current,
          Err(error) => return utils::container::vm::record_task_failure(&key, Err(error), &task_state).await,
        };
        if current.spec.key != revision {
          return Err(IoError::interrupted(
            "VM task",
            "Accepted VM specification was replaced before execution",
          ));
        }
        log::debug!("Executing VM task: {key} {task_action} operation={operation} revision={revision}");
        utils::container::vm::run_task_unlocked(&key, task_action, &task_state).await
      }.await;
      let cleanup = VmDb::delete_mutex(&operation, &task_state).await;
      result.and(cleanup)
    });
    state
      .inner
      .task_manager
      .add_task(&task_key, action.clone(), task, move |error| async move {
        report_task_error(&error_actor, error_action, error, &error_state);
        Ok(())
      })
      .await;
    match tokio::time::timeout(
      std::time::Duration::from_secs(5),
      publish_acceptance(&task_vm, action, operation, &state),
    )
    .await
    {
      Ok(Ok(())) => {}
      Ok(Err(error)) => log::warn!(
        "VM acceptance notification failed: {task_key} operation={operation}: {error}"
      ),
      Err(_) => log::warn!(
        "VM acceptance notification timed out: {task_key} operation={operation}; executing the registered task"
      ),
    }
    let _ = start.send(());
    let _ = reply.send(Ok(vm));
  });
  accepted.await.map_err(|_| {
    HttpError::internal_server_error(
      "VM task acceptance owner stopped unexpectedly",
    )
  })?
}

fn report_task_error(
  actor: &EventActor,
  action: NativeEventAction,
  error: IoError,
  state: &SystemState,
) {
  let action = match action {
    NativeEventAction::Starting => NativeEventAction::Start,
    NativeEventAction::Updating => NativeEventAction::Update,
    NativeEventAction::Destroying => NativeEventAction::Destroy,
    _ => return,
  };
  // The owning task reconciled status before releasing its lifecycle guard.
  // This delayed notification must never mutate a later operation's state.
  state.emit_error_native_action(actor, action, Some(error.to_string()));
}

async fn publish_acceptance(
  vm: &Vm,
  action: NativeEventAction,
  operation: uuid::Uuid,
  state: &SystemState,
) -> IoResult<()> {
  // Dispatch is already complete. Persist and publish this internal notification
  // directly, avoiding a second task without trusting event-supplied markers.
  let event: Event = EventDb::create_try_from(
    EventPartial {
      reporting_controller: vars::CONTROLLER_NAME.to_owned(),
      reporting_node: state.inner.config.hostname.clone(),
      kind: EventKind::Normal,
      action: action.to_string(),
      reason: "state_sync".to_owned(),
      note: Some(format!("Vm {}", vm.spec.vm_key)),
      actor: Some(vm.clone().into()),
      related: None,
      metadata: Some(serde_json::json!({
        "Operation": operation,
        "SpecKey": vm.spec.key,
      })),
    },
    &state.inner.pool,
  )
  .await?
  .try_into()?;
  state.inner.event_emitter_raw.emit(&event)
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

  use crate::models::{
    DistributedMutexDb, DistributedMutexPartial, ObjPsStatusDb,
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
      if action != "stop" {
        let phase_action = match action {
          "start" => NativeEventAction::Starting,
          "update" => NativeEventAction::Updating,
          _ => NativeEventAction::Destroying,
        };
        let rejected = accept_task(&key, phase_action, state, async {
          panic!("another owner's mutex must prevent acceptance mutation");
        })
        .await;
        assert!(rejected.is_err());
      }
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

  #[ntex::test]
  async fn injected_vm_setup_failure_never_mutates_accepted_state() {
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let key = format!("vm-setup-test-{}", uuid::Uuid::new_v4());
    let before = ObjPsStatusDb::create_from(
      ObjPsStatusPartial {
        key: key.clone(),
        actual: ObjPsStatusKind::Start,
        wanted: ObjPsStatusKind::Start,
        prev_actual: ObjPsStatusKind::Stop,
        prev_wanted: ObjPsStatusKind::Stop,
        health: ObjPsHealthStatusKind::Unknown,
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    for action in [
      NativeEventAction::Starting,
      NativeEventAction::Updating,
      NativeEventAction::Destroying,
    ] {
      let polled = std::rc::Rc::new(std::cell::Cell::new(false));
      let prepare_polled = polled.clone();
      let prepare_key = key.clone();
      let prepare_state = state.clone();
      let error = accept_task_with_acquisition(
        &key,
        action,
        state,
        async move {
          prepare_polled.set(true);
          ObjPsStatusDb::update_actual_status(
            &prepare_key,
            &ObjPsStatusKind::Updating,
            &prepare_state.inner.pool,
          )
          .await?;
          Err(HttpError::internal_server_error(
            "preparation unexpectedly ran",
          ))
        },
        async {
          Err(IoError::other(
            "VM task setup",
            "injected acquisition failure",
          ))
        },
      )
      .await
      .unwrap_err();
      assert_eq!(error.status, ntex::http::StatusCode::INTERNAL_SERVER_ERROR);
      assert!(error.to_string().contains("Injected acquisition failure"));
      assert!(!polled.get());
      let after = ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(after.actual, before.actual);
      assert_eq!(after.prev_actual, before.prev_actual);
      assert_eq!(after.wanted, before.wanted);
      assert_eq!(after.updated_at, before.updated_at);
      assert!(
        state
          .inner
          .task_manager
          .get_task(&format!("Vm@{key}"))
          .await
          .is_none()
      );
      let guard = utils::container::vm::lifecycle_lock(&key)
        .try_lock_owned()
        .await
        .unwrap();
      drop(guard);
    }
    ObjPsStatusDb::del_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn vm_start_http_preserves_admission_conflict_and_missing_vm_status() {
    let system = utils::tests::gen_default_test_system().await;
    let key = format!("global.vm-http-status-test-{}", uuid::Uuid::new_v4());
    let guard = utils::container::vm::lifecycle_lock(&key)
      .try_lock_owned()
      .await
      .unwrap();
    for _ in 0..2 {
      let response = system
        .client
        .post(&format!("/processes/vm/{key}/start"))
        .send()
        .await
        .unwrap();
      assert_eq!(response.status(), ntex::http::StatusCode::CONFLICT);
    }
    drop(guard);
    let response = system
      .client
      .post(&format!("/processes/vm/{key}/start"))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), ntex::http::StatusCode::NOT_FOUND);
    assert_eq!(
      DistributedMutexDb::get_lock(
        &VmDb::get_kind(),
        &key,
        &system.state.inner.pool
      )
      .await
      .unwrap_err()
      .inner
      .kind(),
      std::io::ErrorKind::NotFound
    );
  }

  #[ntex::test]
  async fn disconnected_vm_acceptance_finishes_owned_setup_and_recovery() {
    use futures::StreamExt;
    use nanocl_stubs::system::EventCondition;
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let key = format!("global.vm-disconnect-test-{}", uuid::Uuid::new_v4());
    ObjPsStatusDb::create_from(
      ObjPsStatusPartial {
        key: key.clone(),
        actual: ObjPsStatusKind::Stop,
        wanted: ObjPsStatusKind::Stop,
        prev_actual: ObjPsStatusKind::Stop,
        prev_wanted: ObjPsStatusKind::Stop,
        health: ObjPsHealthStatusKind::Unknown,
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let mut registration = state
      .subscribe_raw(Some(vec![EventCondition {
        actor_key: Some(key.clone()),
        actor_kind: Some(EventActorKind::Vm),
        kind: vec![EventKind::Normal],
        action: vec![NativeEventAction::Starting],
        ..Default::default()
      }]))
      .await
      .unwrap();
    let (acquiring, acquisition_entered) = tokio::sync::oneshot::channel();
    let (release, acquire_gate) = tokio::sync::oneshot::channel();
    let request_key = key.clone();
    let request_state = state.clone();
    let acquire_key = key.clone();
    let acquire_state = state.clone();
    let prepare_key = key.clone();
    let prepare_state = state.clone();
    let caller = ntex::rt::spawn(async move {
      accept_task_with_acquisition(
        &request_key,
        NativeEventAction::Starting,
        &request_state,
        async move {
          let status = ObjPsStatusDb::update_actual_status(
            &prepare_key,
            &ObjPsStatusKind::Starting,
            &prepare_state.inner.pool,
          )
          .await;
          status?;
          let vm = Vm {
            namespace_name: "global".to_owned(),
            spec: nanocl_stubs::vm_spec::VmSpec {
              vm_key: prepare_key,
              key: uuid::Uuid::new_v4(),
              ..Default::default()
            },
            status: nanocl_stubs::system::ObjPsStatus {
              actual: ObjPsStatusKind::Starting,
              ..Default::default()
            },
            ..Default::default()
          };
          // Deliberately omit the VM row: execution must reconcile this accepted
          // transition after its initial read fails, without creating a runtime.
          Ok(vm)
        },
        async move {
          acquiring.send(()).unwrap();
          acquire_gate.await.unwrap();
          VmDb::create_mutex(&acquire_key, "start", &acquire_state).await
        },
      )
      .await
    });
    acquisition_entered.await.unwrap();
    caller.cancel();
    // The HTTP-side future is gone, but its detached acceptance owner still
    // excludes new mutations until setup and failure recovery are complete.
    assert!(
      utils::container::vm::lifecycle_lock(&key)
        .try_lock_owned()
        .await
        .is_err()
    );
    assert_eq!(
      ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap()
        .actual,
      ObjPsStatusKind::Stop.to_string()
    );
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
      loop {
        let bytes = registration.next().await.unwrap().unwrap();
        if bytes.is_empty() {
          continue;
        }
        let event: Event = serde_json::from_slice(&bytes).unwrap();
        if event.kind == EventKind::Normal
          && event.action == NativeEventAction::Starting.to_string()
          && event.actor.as_ref().and_then(|actor| actor.key.as_deref())
            == Some(key.as_str())
        {
          break;
        }
      }
      state
        .inner
        .task_manager
        .wait_task(&format!("Vm@{key}"))
        .await;
    })
    .await
    .unwrap();
    assert_eq!(
      ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap()
        .actual,
      ObjPsStatusKind::Fail.to_string()
    );
    let missing =
      DistributedMutexDb::get_lock(&VmDb::get_kind(), &key, &state.inner.pool)
        .await
        .unwrap_err();
    assert_eq!(missing.inner.kind(), std::io::ErrorKind::NotFound);
    let guard = utils::container::vm::lifecycle_lock(&key)
      .try_lock_owned()
      .await
      .unwrap();
    drop(guard);
    ObjPsStatusDb::del_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
  }

  #[ntex::test]
  async fn delayed_vm_failure_notification_preserves_newer_operation() {
    use futures::StreamExt;
    use nanocl_stubs::system::EventCondition;
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let key = format!("vm-callback-test-{}", uuid::Uuid::new_v4());
    let status = ObjPsStatusDb::create_from(
      ObjPsStatusPartial {
        key: key.clone(),
        actual: ObjPsStatusKind::Updating,
        wanted: ObjPsStatusKind::Start,
        prev_actual: ObjPsStatusKind::Start,
        prev_wanted: ObjPsStatusKind::Start,
        health: ObjPsHealthStatusKind::Unknown,
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let owner = VmDb::create_mutex(&key, "update", state).await.unwrap();
    let mut events = state
      .subscribe_raw(Some(vec![EventCondition {
        actor_key: Some(key.clone()),
        actor_kind: Some(EventActorKind::Vm),
        kind: vec![EventKind::Error],
        action: vec![NativeEventAction::Start],
        ..Default::default()
      }]))
      .await
      .unwrap();
    let actor = EventActor {
      key: Some(key.clone()),
      kind: EventActorKind::Vm,
      attributes: None,
    };
    report_task_error(
      &actor,
      NativeEventAction::Starting,
      IoError::other("VM task", "older setup failed"),
      state,
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
      loop {
        let bytes = events.next().await.unwrap().unwrap();
        if bytes.is_empty() {
          continue;
        }
        let event: Event = serde_json::from_slice(&bytes).unwrap();
        if event.kind == EventKind::Error
          && event.actor.as_ref().and_then(|actor| actor.key.as_deref())
            == Some(key.as_str())
        {
          break;
        }
      }
    })
    .await
    .unwrap();
    let after = ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
    let remaining =
      DistributedMutexDb::get_lock(&VmDb::get_kind(), &key, &state.inner.pool)
        .await
        .unwrap();
    assert_eq!(after.actual, status.actual);
    assert_eq!(after.prev_actual, status.prev_actual);
    assert_eq!(after.updated_at, status.updated_at);
    assert_eq!(remaining.key, owner.key);
    VmDb::delete_mutex(&owner.key, state).await.unwrap();
    ObjPsStatusDb::del_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
  }
}
