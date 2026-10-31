use std::{future::Future, sync::Arc};

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::{
  process::ProcessKind,
  system::{
    Event, EventActor, EventActorKind, EventKind, EventPartial,
    NativeEventAction, ObjPsStatusKind,
  },
  vm::Vm,
};

use crate::{
  models::{
    AcceptedVmOperation, DistributedMutexDb, EventDb, OperationDb,
    OperationOwner, SystemState, VmDb, VmOperationLease, VmRestartTask,
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
  accept_task_with_receipt(key, action, state, prepare)
    .await
    .map(|(vm, _)| vm)
}

/// Surface the UUID captured at admission, never a later current-operation read.
pub(crate) async fn start_operation(
  key: &str,
  state: &SystemState,
) -> HttpResult<Option<uuid::Uuid>> {
  let prepare_key = key.to_owned();
  let prepare_state = state.clone();
  let acquire_key = key.to_owned();
  let acquire_state = state.clone();
  accept_task_with_acquisition(
    key,
    NativeEventAction::Starting,
    state,
    move |owner, lease| async move {
      VmDb::prepare_start(&prepare_key, owner, lease, &prepare_state).await
    },
    async move {
      VmDb::create_mutex(&acquire_key, "start", &acquire_state).await
    },
    None,
    true,
  )
  .await
  .map(|(_, operation)| operation)
}

pub(crate) async fn accept_task_with_receipt<P, F>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: P,
) -> HttpResult<(Vm, uuid::Uuid)>
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
  accept_task_with_acquisition(
    key,
    action,
    state,
    prepare,
    async move {
      VmDb::create_mutex(&acquire_key, mutex_action, &acquire_state).await
    },
    None,
    false,
  )
  .await
  .and_then(|(vm, operation)| {
    operation.map(|operation| (vm, operation)).ok_or_else(|| {
      HttpError::internal_server_error("Accepted VM task has no operation")
    })
  })
}

pub(crate) async fn accept_restart(
  key: &str,
  restart: VmRestartTask,
  state: &SystemState,
) -> HttpResult<Vm> {
  let prepare_key = key.to_owned();
  let prepare_state = state.clone();
  let acquire_key = key.to_owned();
  let acquire_state = state.clone();
  accept_task_with_acquisition(
    key,
    NativeEventAction::Restart,
    state,
    move |owner, lease| async move {
      let vm =
        VmDb::transform_read_by_pk(&prepare_key, &prepare_state.inner.pool)
          .await?;
      let operation =
        OperationDb::new(&owner, "Vm", &prepare_key, vm.spec.key, "restart");
      VmDb::accept_operation(
        &vm,
        None,
        operation,
        lease,
        &prepare_state.inner.pool,
      )
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)
    },
    async move {
      VmDb::create_mutex(&acquire_key, "restart", &acquire_state).await
    },
    Some(restart),
    false,
  )
  .await
  .map(|(vm, _)| vm)
}

async fn accept_task_with_acquisition<P, F, A>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: P,
  acquire: A,
  restart: Option<VmRestartTask>,
  allow_unchanged_start: bool,
) -> HttpResult<(Vm, Option<uuid::Uuid>)>
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
    if allow_unchanged_start {
      let unchanged = async {
        if OperationDb::current("Vm", &key, &state.inner.pool)
          .await?
          .is_some_and(|operation| operation.outcome.is_none())
        {
          return Err(HttpError::conflict(
            "A VM lifecycle operation is awaiting completion or recovery",
          ));
        }
        let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
        if utils::container::vm::state_is_unchanged(
          &vm,
          ObjPsStatusKind::Start,
          &state,
        )
        .await?
        {
          let (current, _, _) =
            VmDb::read_by_pk(&key, &state.inner.pool).await?;
          if current.incarnation_key != owner.incarnation_key
            || current.spec_key != vm.spec.key
          {
            return Err(HttpError::conflict(
              "VM identity or state changed during start admission",
            ));
          }
          Ok(Some(vm))
        } else {
          Ok(None)
        }
      }
      .await;
      if let Some(unchanged) = unchanged.transpose() {
        // No durable acceptance exists. Remove temporary admission evidence
        // and release both leases before returning an unchanged response.
        let cleanup = async {
          if !utils::operation::cleanup_owner(&lease.1, &state.inner.pool)
            .await?
          {
            return Err(IoError::interrupted(
              "VM start admission",
              "Temporary operation owner could not be released",
            ));
          }
          VmDb::delete_mutex(&mutex.key, &state).await
        }
        .await;
        drop(lease);
        let result = match unchanged {
          Ok(vm) => cleanup.map(|()| (vm, None)).map_err(Into::into),
          Err(error) => {
            if let Err(cleanup) = cleanup {
              log::warn!(
                "VM unchanged admission cleanup failed: {key}: {cleanup}"
              );
            }
            Err(error)
          }
        };
        let _ = reply.send(result);
        return;
      }
    }
    let accepted_operation = match prepare(owner, lease.clone()).await {
      Ok(accepted) => accepted,
      Err(error) => {
        // Fence against a blocking acceptance transaction before treating
        // absence as rollback. Unknown or committed state stays recoverable.
        match utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
        {
          Ok(true) => {
            if let Err(cleanup) = VmDb::delete_mutex(&mutex.key, &state).await {
              log::error!(
                "VM acceptance cleanup failed: {key} {}: {cleanup}",
                mutex.key
              );
            }
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
    let completion_actor: EventActor = vm.clone().into();
    let completion = completion_action(&action);
    let error_state = state.clone();
    let error_key = key.clone();
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
      let (mut result, settled) = match admission {
        Ok(()) => {
          log::debug!(
            "Executing VM task: {key} {task_action} operation={operation} revision={revision}"
          );
          if let Some(restart) = &restart {
            utils::container::vm::run_restart_owned(
              &key,
              &restart.options,
              restart.expected_id.as_deref(),
              &accepted_operation,
              &lease,
              &task_state,
            )
            .await
          } else {
            utils::container::vm::run_task_unlocked(
              &key,
              task_action,
              &accepted_operation,
              &lease,
              &task_state,
            )
            .await
          }
        }
        Err(error) => (Err(error), false),
      };
      // Runtime helpers settle ordinary outcomes under this same owner. A
      // still-pending row is retained for evidence-based recovery after exit.
      // Successful deletion already committed its exact operation together
      // with removing the live row; it cannot be completed a second time.
      let mut completed =
        accepted_operation.action == "delete" && result.is_ok();
      if settled && !completed {
        match VmDb::complete_operation(
          &accepted_operation,
          if result.is_ok() { "completed" } else { "failed" },
          result.as_ref().err().map(|_| serde_json::json!({
            "Failure": "VM execution failed; inspect the corresponding VM error event",
            "Operation": operation,
            "Stage": "runtime-execution"
          })),
          lease.clone(),
          &task_state.inner.pool,
        ).await {
          Ok(committed) => completed = committed,
          Err(error) => log::error!(
            "VM operation completion persistence failed: {key} operation={operation}: {error}"
          ),
        }
      }
      if completion.is_some() {
        result = require_committed_result(result, completed);
      }
      if let Err(error) =
        utils::operation::cleanup_owner(&lease.1, &task_state.inner.pool).await
      {
        log::warn!("VM owner file cleanup deferred: {key}: {error}");
      }
      let cleanup = VmDb::delete_mutex(&operation, &task_state).await;
      if let Err(error) = &cleanup {
        log::warn!(
          "VM mutex cleanup deferred: {key} operation={operation}: {error}"
        );
      }
      let result = result_after_cleanup(result, completed, cleanup);
      // Preserve the synchronous restart endpoint without waking its caller
      // while the mutation/owner lease is still held.
      drop(lease);
      if let Some(action) = completion {
        let (action, kind, reason, note) =
          completion_notification(action, &result, completed, &key);
        task_state
          .emit_action_sync(
            &completion_actor,
            action,
            kind,
            reason,
            Some(note),
            Some(
              serde_json::json!({"Operation": operation, "SpecKey": revision}),
            ),
          )
          .await;
        // The notification owns this operation result. Returning its error
        // would make TaskManager publish it a second time.
        return Ok(());
      }
      if let Some(restart) = restart {
        let _ = restart.completion.send(result.clone());
      }
      result
    });
    state
      .inner
      .task_manager
      .add_task(&task_key, action.clone(), task, move |error| async move {
        if let Some(action) = completion_action(&error_action) {
          // An owner exit cannot establish a durable operation outcome.
          let (action, kind, reason, note) =
            completion_notification(action, &Err(error), false, &error_key);
          error_state.emit_action(
            &error_actor,
            action,
            kind,
            reason,
            Some(note),
            Some(
              serde_json::json!({"Operation": operation, "SpecKey": revision}),
            ),
          );
        } else {
          report_task_error(&error_actor, error_action, error, &error_state);
        }
        Ok(())
      })
      .await;
    // Restart has no separate acceptance event. Its existing Restart event
    // remains a terminal notification, emitted only after runtime confirmation.
    if action != NativeEventAction::Restart {
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
    }
    let _ = start.send(());
    let _ = reply.send(Ok((vm, Some(operation))));
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

fn completion_action(action: &NativeEventAction) -> Option<NativeEventAction> {
  match action {
    NativeEventAction::Starting => Some(NativeEventAction::Start),
    NativeEventAction::Updating => Some(NativeEventAction::Update),
    NativeEventAction::Destroying => Some(NativeEventAction::Destroy),
    _ => None,
  }
}

fn completion_notification(
  action: NativeEventAction,
  result: &IoResult<()>,
  completed: bool,
  key: &str,
) -> (NativeEventAction, EventKind, &'static str, String) {
  if !completed {
    return (
      NativeEventAction::Other("operation_pending".to_owned()),
      EventKind::Warning,
      "operation_pending",
      format!(
        "VM {key} operation outcome is unknown and may remain pending recovery; inspect the runtime and operation before retrying"
      ),
    );
  }
  let (kind, note) = match result {
    Ok(()) => (EventKind::Normal, format!("Vm {key}")),
    Err(error) => (EventKind::Error, error.to_string()),
  };
  (action, kind, "state_sync", note)
}

fn require_committed_result(
  result: IoResult<()>,
  completed: bool,
) -> IoResult<()> {
  result?;
  if !completed {
    return Err(IoError::interrupted(
      "VM operation",
      "Runtime work finished but operation completion was not recorded",
    ));
  }
  Ok(())
}

fn result_after_cleanup(
  result: IoResult<()>,
  completed: bool,
  cleanup: IoResult<()>,
) -> IoResult<()> {
  // Cleanup cannot rewrite an already committed operation outcome. Retain its
  // error for an uncommitted synchronous operation that has no earlier failure.
  if completed {
    result
  } else {
    result.and(cleanup)
  }
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
    NativeEventAction::Restart => NativeEventAction::Restart,
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
  #[test]
  fn vm_receipts_use_terminal_actions() {
    for (acceptance, terminal) in [
      (NativeEventAction::Starting, NativeEventAction::Start),
      (NativeEventAction::Updating, NativeEventAction::Update),
      (NativeEventAction::Destroying, NativeEventAction::Destroy),
    ] {
      assert_eq!(super::completion_action(&acceptance), Some(terminal));
    }
    assert_eq!(super::completion_action(&NativeEventAction::Restart), None);
  }

  #[test]
  fn vm_completion_notifications_distinguish_pending_operations() {
    for action in [
      NativeEventAction::Start,
      NativeEventAction::Update,
      NativeEventAction::Destroy,
    ] {
      for result in [Ok(()), Err(IoError::other("runtime", "execution failed"))]
      {
        let (pending_action, kind, reason, note) =
          completion_notification(action.clone(), &result, false, "global.vm");
        assert_eq!(
          pending_action,
          NativeEventAction::Other("operation_pending".to_owned())
        );
        assert_eq!(kind, EventKind::Warning);
        assert_eq!(reason, "operation_pending");
        assert!(note.contains("outcome is unknown"));
        assert!(
          note.contains("inspect the runtime and operation before retrying")
        );

        let (terminal_action, kind, reason, note) =
          completion_notification(action.clone(), &result, true, "global.vm");
        assert_eq!(terminal_action, action);
        assert_eq!(
          kind,
          if result.is_ok() {
            EventKind::Normal
          } else {
            EventKind::Error
          }
        );
        assert_eq!(reason, "state_sync");
        assert_eq!(
          note,
          match &result {
            Ok(()) => "Vm global.vm".to_owned(),
            Err(error) => error.to_string(),
          }
        );
      }
    }
  }

  #[test]
  fn vm_completion_requires_durable_settlement_before_success() {
    assert!(super::require_committed_result(Ok(()), true).is_ok());
    let incomplete =
      super::require_committed_result(Ok(()), false).unwrap_err();
    assert_eq!(incomplete.inner.kind(), std::io::ErrorKind::Interrupted);
    for completed in [false, true] {
      let error = super::require_committed_result(
        Err(nanocl_error::io::IoError::other("runtime", "start failed")),
        completed,
      )
      .unwrap_err();
      assert_eq!(error.context(), Some("runtime"));
      assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    }
  }

  #[test]
  fn vm_cleanup_failure_preserves_committed_operation_outcomes() {
    let cleanup_failure =
      || Err(IoError::other("cleanup", "mutex unavailable"));
    let succeeded = result_after_cleanup(Ok(()), true, cleanup_failure());
    assert!(succeeded.is_ok());
    assert_eq!(
      completion_notification(NativeEventAction::Start, &succeeded, true, "vm")
        .1,
      EventKind::Normal
    );
    for completed in [false, true] {
      let failed = result_after_cleanup(
        Err(IoError::other("runtime", "execution failed")),
        completed,
        cleanup_failure(),
      );
      assert_eq!(failed.as_ref().unwrap_err().context(), Some("runtime"));
      assert_eq!(
        completion_notification(
          NativeEventAction::Start,
          &failed,
          completed,
          "vm"
        )
        .1,
        if completed {
          EventKind::Error
        } else {
          EventKind::Warning
        }
      );
    }
    let pending = result_after_cleanup(
      require_committed_result(Ok(()), false),
      false,
      cleanup_failure(),
    );
    assert_eq!(
      pending.as_ref().unwrap_err().context(),
      Some("VM operation")
    );
    assert_eq!(
      completion_notification(NativeEventAction::Start, &pending, false, "vm")
        .2,
      "operation_pending"
    );
    // The synchronous restart path still reports cleanup errors when durable
    // completion was not confirmed and execution supplied no earlier error.
    assert_eq!(
      result_after_cleanup(Ok(()), false, cleanup_failure())
        .unwrap_err()
        .context(),
      Some("cleanup")
    );
  }

  use nanocl_stubs::system::{
    ObjPsHealthStatusKind, ObjPsStatusKind, ObjPsStatusPartial,
  };

  use crate::models::{
    DistributedMutexDb, DistributedMutexPartial, ObjPsStatusDb,
  };

  use super::*;

  #[ntex::test]
  async fn vm_owner_exit_invokes_task_fallback_once() {
    let manager = crate::models::TaskManager::default();
    let failures = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let reported = failures.clone();
    let (release, blocked) = tokio::sync::oneshot::channel();
    let task = owned_executor(async move {
      blocked.await.unwrap();
      // The detached owner exits without sending an operation result.
      panic!("injected VM execution owner exit");
    });
    manager
      .add_task(
        "Vm@owner-exit-test",
        NativeEventAction::Starting,
        task,
        move |error| async move {
          reported.borrow_mut().push(error);
          Ok(())
        },
      )
      .await;
    let registered = manager.get_task("Vm@owner-exit-test").await.unwrap();
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), registered.wait())
      .await
      .expect("VM owner exit did not reach the task fallback");
    assert!(manager.get_task("Vm@owner-exit-test").await.is_none());
    let failures = failures.borrow();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].context(), Some("VM task"));
    assert_eq!(failures[0].inner.kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(
      failures[0].inner.to_string(),
      "Execution owner stopped unexpectedly"
    );
  }

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
        None,
        false,
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
