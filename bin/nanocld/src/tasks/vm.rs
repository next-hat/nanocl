use std::{future::Future, sync::Arc};

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
  models::{
    AcceptedVmOperation, DistributedMutexDb, EventDb, OperationDb,
    OperationOwner, SystemState, VmDb, VmOperationLease,
  },
  repositories::generic::*,
  utils, vars,
};

use super::generic::*;

// Acceptance owns both locks before polling the desired/status mutation. The
// detached owner hands them directly to the registered task, so disconnects or
// event delivery failures cannot strand a live pending transition.
pub(crate) async fn accept_task<P, F>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: P,
) -> HttpResult<Vm>
where
  P: FnOnce(OperationOwner, VmOperationLease) -> F + 'static,
  F: Future<Output = HttpResult<AcceptedVmOperation>> + 'static,
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

async fn accept_task_with_acquisition<P, F, A>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: P,
  acquire: A,
) -> HttpResult<Vm>
where
  P: FnOnce(OperationOwner, VmOperationLease) -> F + 'static,
  F: Future<Output = HttpResult<AcceptedVmOperation>> + 'static,
  A: Future<Output = IoResult<DistributedMutexDb>> + 'static,
{
  let guard = utils::container::vm::lifecycle_lock(key)
    .try_lock_owned()
    .await
    .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
  let guard = Arc::new(guard);
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
    let live = match VmDb::read_by_pk(&key, &state.inner.pool).await {
      Ok((live, _, _)) => live,
      Err(error) => {
        let _ = VmDb::delete_mutex(&mutex.key, &state).await;
        let _ = reply.send(Err(error.into()));
        return;
      }
    };
    let (owner, owner_guard) = match utils::operation::create_owner(
      mutex.key,
      "Vm",
      &key,
      live.incarnation_key,
      &state.inner.config.hostname,
      &state.inner.config.state_dir,
    ) {
      Ok(owner) => owner,
      Err(error) => {
        if let Err(cleanup) = VmDb::delete_mutex(&mutex.key, &state).await {
          log::error!(
            "VM acceptance cleanup failed: {key} {}: {cleanup}",
            mutex.key
          );
        }
        let _ = reply.send(Err(error.into()));
        return;
      }
    };
    let lease = (guard, owner_guard);
    let accepted_operation = match prepare(owner, lease.clone()).await {
      Ok(accepted) => accepted,
      Err(error) => {
        // Fence against a blocking acceptance transaction before treating
        // absence as rollback. Unknown or committed state stays recoverable.
        match utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
        {
          Ok(true) => {
            let _ = VmDb::delete_mutex(&mutex.key, &state).await;
          }
          Ok(false) => {}
          Err(cleanup) => {
            log::warn!("VM acceptance cleanup deferred: {key}: {cleanup}")
          }
        }
        let _ = reply.send(Err(error));
        return;
      }
    };
    let vm = accepted_operation.vm;
    let accepted_operation = accepted_operation.operation;
    let revision = vm.spec.key;
    let operation = accepted_operation.key;
    let task_state = state.clone();
    let task_action = action.clone();
    let task_vm = vm.clone();
    let error_state = state.clone();
    let error_action = action.clone();
    let error_actor: EventActor = vm.clone().into();
    let (start, ready) = tokio::sync::oneshot::channel();
    let task = owned_executor(async move {
      let lease = lease;
      // The task is registered before its acceptance notification, but cannot
      // finish before subscribers have been told it was registered.
      let _ = ready.await;
      let admission = async {
        let accepted =
          OperationDb::current("Vm", &key, &task_state.inner.pool).await?;
        let current =
          VmDb::transform_read_by_pk(&key, &task_state.inner.pool).await?;
        let (live, _, _) =
          VmDb::read_by_pk(&key, &task_state.inner.pool).await?;
        if live.incarnation_key != accepted_operation.incarnation_key
          || current.spec.key != revision
          || accepted.as_ref() != Some(&accepted_operation)
        {
          return Err(IoError::interrupted(
            "VM task",
            "Accepted VM specification was replaced before execution",
          ));
        }
        Ok(())
      }
      .await;
      let (result, settled) = match admission {
        Ok(()) => {
          log::debug!(
            "Executing VM task: {key} {task_action} operation={operation} revision={revision}"
          );
          utils::container::vm::run_task_unlocked(
            &key,
            task_action,
            &accepted_operation,
            &lease,
            &task_state,
          )
          .await
        }
        Err(error) => (Err(error), false),
      };
      // Runtime helpers settle ordinary outcomes under this same owner. A
      // still-pending row is retained for evidence-based recovery after exit.
      if settled
        && let Err(error) = VmDb::complete_operation(
            &accepted_operation,
            if result.is_ok() {
              "completed"
            } else {
              "failed"
            },
            result.as_ref().err().map(|_| serde_json::json!({
              "Failure": "VM execution failed; inspect the corresponding VM error event",
              "Operation": operation,
              "Stage": "runtime-execution"
            })),
            lease.clone(),
            &task_state.inner.pool,
          )
          .await
      {
        log::error!(
          "VM operation completion persistence failed: {key} operation={operation}: {error}"
        );
      }
      if let Err(error) =
        utils::operation::cleanup_owner(&lease.1, &task_state.inner.pool).await
      {
        log::warn!("VM owner file cleanup deferred: {key}: {error}");
      }
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

// TaskManager owns only this waiter. Cancelling it cannot drop the execution
// lease while a runtime helper is awaiting a blocking database write.
fn owned_executor(
  future: impl Future<Output = IoResult<()>> + 'static,
) -> ObjTaskFuture {
  let (send, receive) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let _ = send.send(future.await);
  });
  Box::pin(async move {
    receive.await.map_err(|_| {
      IoError::interrupted("VM task", "Execution owner stopped unexpectedly")
    })?
  })
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
  async fn cancelled_vm_waiter_retains_executor_ownership_until_completion() {
    let key = format!("global.vm-waiter-{}", uuid::Uuid::new_v4());
    let directory =
      std::env::temp_dir().join(format!("vm-waiter-{}", uuid::Uuid::new_v4()));
    let lifecycle = std::sync::Arc::new(
      utils::container::vm::lifecycle_lock(&key)
        .try_lock_owned()
        .await
        .unwrap(),
    );
    let (owner, guard) = utils::operation::create_owner(
      uuid::Uuid::new_v4(),
      "Vm",
      &key,
      uuid::Uuid::new_v4(),
      "test-node",
      directory.to_str().unwrap(),
    )
    .unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, proceed) = tokio::sync::oneshot::channel();
    let (finished, completed) = tokio::sync::oneshot::channel();
    let task = owned_executor(async move {
      let lease = (lifecycle, guard);
      entered.send(()).unwrap();
      proceed.await.unwrap();
      drop(lease);
      finished.send(()).unwrap();
      Ok(())
    });
    let manager = crate::models::TaskManager::default();
    manager
      .add_task(&key, NativeEventAction::Updating, task, |_| async {
        Ok(())
      })
      .await;
    started.await.unwrap();
    manager.remove_task(&key).await;
    assert!(manager.get_task(&key).await.is_none());
    assert!(
      utils::container::vm::lifecycle_lock(&key)
        .try_lock_owned()
        .await
        .is_err()
    );
    assert!(
      utils::operation::abandoned_owner(
        &owner,
        "test-node",
        directory.to_str().unwrap()
      )
      .unwrap()
      .is_none()
    );
    release.send(()).unwrap();
    completed.await.unwrap();
    assert!(
      utils::container::vm::lifecycle_lock(&key)
        .try_lock_owned()
        .await
        .is_ok()
    );
    let recovered = utils::operation::abandoned_owner(
      &owner,
      "test-node",
      directory.to_str().unwrap(),
    )
    .unwrap();
    assert!(recovered.is_some());
    drop(recovered);
    std::fs::remove_dir_all(directory).unwrap();
  }

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
        let rejected = accept_task(&key, phase_action, state, |_, _| async {
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
        move |_, _| async move {
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

  async fn acceptance_fixture(state: &SystemState) -> Vm {
    use crate::objects::generic::ObjCreate;
    VmDb::fn_create_obj(
      &crate::models::VmObjCreateIn {
        namespace: "global".into(),
        version: vars::VERSION.into(),
        spec: nanocl_stubs::vm_spec::VmSpecPartial {
          name: format!("acceptance-{}", uuid::Uuid::new_v4().simple()),
          // A missing bind source may be created as a directory by Docker.
          // A path below a character device deterministically rejects creation.
          image: "/dev/null/nanocl-acceptance-test.img".into(),
          cloud_init: Some(false),
          host_config: Some(nanocl_stubs::vm_spec::VmHostConfig {
            runtime_network: Some("bridge".into()),
            ..Default::default()
          }),
          ..Default::default()
        },
      },
      state,
    )
    .await
    .unwrap()
  }

  #[ntex::test]
  async fn rejected_vm_transaction_registers_no_executor_and_releases_only_its_admission()
   {
    use diesel::RunQueryDsl;
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let vm = acceptance_fixture(state).await;
    let key = vm.spec.vm_key.clone();
    let original = ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
    let mut spec = crate::models::SpecDb::try_from_vm_partial(
      &key,
      &vm.spec.version,
      &nanocl_stubs::vm_spec::VmSpecPartial::from(vm.spec.clone()),
    )
    .unwrap();
    let revision = spec.key;
    let foreign_key =
      format!("global.other-admission-{}", uuid::Uuid::new_v4());
    let foreign = VmDb::create_mutex(&foreign_key, "update", state)
      .await
      .unwrap();
    let prepare_state = state.clone();
    let expected = vm.clone();
    let error = accept_task(&key, NativeEventAction::Updating, state, move |owner, lease| async move {
      spec.incarnation_key = Some(owner.incarnation_key);
      let operation = OperationDb::new(&owner, "Vm", &expected.spec.vm_key, revision, "update");
      Ok(VmDb::accept_operation_with_pending_write(&expected, Some(spec), operation, lease,
        &prepare_state.inner.pool, |conn, key, _| {
          diesel::sql_query("UPDATE object_process_statuses SET actual = NULL WHERE key = $1")
            .bind::<diesel::sql_types::Text,_>(key).execute(conn)?;
          unreachable!("pending status is NOT NULL")
        }).await?)
    }).await.unwrap_err();
    assert!(error.to_string().to_ascii_lowercase().contains("null"));
    assert_eq!(
      VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap()
        .spec
        .key,
      vm.spec.key
    );
    let after = ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
    assert_eq!(after.actual, original.actual);
    assert_eq!(after.wanted, original.wanted);
    assert_eq!(after.updated_at, original.updated_at);
    assert!(
      crate::models::SpecDb::read_by_pk(&revision, &state.inner.pool)
        .await
        .is_err()
    );
    assert!(
      OperationDb::current("Vm", &key, &state.inner.pool)
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      state
        .inner
        .task_manager
        .get_task(&format!("Vm@{key}"))
        .await
        .is_none()
    );
    assert!(
      DistributedMutexDb::get_lock(&VmDb::get_kind(), &key, &state.inner.pool)
        .await
        .is_err()
    );
    assert_eq!(
      DistributedMutexDb::read_by_pk(&foreign.key, &state.inner.pool)
        .await
        .unwrap()
        .key,
      foreign.key
    );
    let guard = utils::container::vm::lifecycle_lock(&key)
      .try_lock_owned()
      .await
      .unwrap();
    drop(guard);
    VmDb::delete_mutex(&foreign.key, state).await.unwrap();
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
  }

  #[ntex::test]
  async fn disconnected_vm_acceptance_finishes_owned_setup_and_recovery() {
    use futures::StreamExt;
    use nanocl_stubs::system::EventCondition;
    let system = utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let vm = acceptance_fixture(state).await;
    let key = vm.spec.vm_key.clone();
    let revision = vm.spec.key;
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
    let (committed, commit_seen) = tokio::sync::oneshot::channel();
    let (release, gate) = tokio::sync::oneshot::channel();
    let request_key = key.clone();
    let request_state = state.clone();
    let prepare_state = state.clone();
    let caller = ntex::rt::spawn(async move {
      accept_task(
        &request_key,
        NativeEventAction::Starting,
        &request_state,
        move |owner, lease| async move {
          let operation = OperationDb::new(
            &owner,
            "Vm",
            &vm.spec.vm_key,
            vm.spec.key,
            "start",
          );
          let accepted = VmDb::accept_operation(
            &vm,
            None,
            operation,
            lease,
            &prepare_state.inner.pool,
          )
          .await?;
          committed.send(()).unwrap();
          gate.await.unwrap();
          Ok(accepted)
        },
      )
      .await
    });
    commit_seen.await.unwrap();
    caller.cancel();
    let operation = OperationDb::current("Vm", &key, &state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(operation.spec_key, revision);
    assert_eq!(operation.action, "start");
    assert!(
      state
        .inner
        .task_manager
        .get_task(&format!("Vm@{key}"))
        .await
        .is_none()
    );
    assert!(
      !utils::container::vm::recovery::recover_one(&operation, state)
        .await
        .unwrap()
    );
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
      "starting"
    );
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      loop {
        let bytes = registration.next().await.unwrap().unwrap();
        if bytes.is_empty() {
          continue;
        }
        let event: Event = serde_json::from_slice(&bytes).unwrap();
        if event.kind == EventKind::Normal && event.action == "starting" {
          let metadata = event.metadata.unwrap();
          assert_eq!(metadata["Operation"], serde_json::json!(operation.key));
          assert_eq!(metadata["SpecKey"], serde_json::json!(revision));
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
    let completed = OperationDb::current("Vm", &key, &state.inner.pool)
      .await
      .unwrap()
      .unwrap();
    assert_eq!(completed.key, operation.key);
    assert_eq!(completed.spec_key, revision);
    // The invalid bind source fails execution after the exact accepted revision
    // was checked; normal failure reconciliation observes no running runtime.
    assert_eq!(completed.outcome.as_deref(), Some("failed"));
    assert_eq!(
      ObjPsStatusDb::read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap()
        .actual,
      "stop"
    );
    assert!(
      DistributedMutexDb::get_lock(&VmDb::get_kind(), &key, &state.inner.pool)
        .await
        .is_err()
    );
    let guard = utils::container::vm::lifecycle_lock(&key)
      .try_lock_owned()
      .await
      .unwrap();
    let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
      .await
      .unwrap();
    if let Some(runtime) = utils::container::vm::inspect_runtime(&vm, state)
      .await
      .unwrap()
    {
      assert_eq!(runtime.state.and_then(|state| state.running), Some(false));
    }
    drop(guard);
    utils::container::vm::delete(&key, state).await.unwrap();
    assert!(
      utils::container::vm::inspect_runtime(&vm, state)
        .await
        .unwrap()
        .is_none()
    );
    assert!(
      crate::models::ProcessDb::read_by_kind_key(&key, None, &state.inner.pool)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      !std::path::Path::new(&state.inner.config.state_dir)
        .join("run/operations")
        .join(format!("{}.lock", operation.key))
        .exists()
    );
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
