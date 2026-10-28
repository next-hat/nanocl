use std::{collections::BTreeMap, future::Future};

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::cargo::Cargo;
use nanocl_stubs::{
  node::CargoReplicaTask,
  system::{
    Event, EventActorKind, EventKind, EventPartial, NativeEventAction,
    ObjPsHealthStatusKind, ObjPsStatusKind,
  },
};

use crate::{
  models::{
    AcceptedCargoOperation, CargoDb, CargoOperationLease,
    CargoReplicaAssignment, CargoReplicaDb, CargoRuntimeContext, EventDb,
    NodeDb, ObjPsStatusDb, ObjPsStatusUpdate, OperationDb, OperationOwner,
    ProcessDb, SystemState,
  },
  repositories::generic::*,
  utils::{self, container::generic::SelectionPlan},
};

use super::generic::*;

impl ObjTask for CargoDb {
  fn get_kind() -> String {
    EventActorKind::Cargo.to_string()
  }
}

fn group_assigned_replicas_by_node(
  cargo_key: &str,
  assignments: &[CargoReplicaAssignment],
  selection: &SelectionPlan,
) -> Result<Vec<(NodeDb, Vec<CargoReplicaTask>)>, IoError> {
  let mut replicas_by_node = BTreeMap::<String, Vec<CargoReplicaTask>>::new();
  for assignment in assignments {
    if let Some(node_name) = &assignment.node_name {
      replicas_by_node.entry(node_name.clone()).or_default().push(
        CargoReplicaTask {
          key: assignment.replica_key,
          ordinal: assignment.ordinal,
        },
      );
    }
  }
  for replicas in replicas_by_node.values_mut() {
    replicas.sort_by_key(|replica| replica.ordinal);
  }
  let dispatches = selection
    .assignments
    .iter()
    .filter_map(|assignment| {
      let replicas = replicas_by_node.remove(&assignment.node.name)?;
      Some((assignment.node.clone(), replicas))
    })
    .collect::<Vec<_>>();
  if let Some((node_name, _)) = replicas_by_node.into_iter().next() {
    return Err(IoError::other(
      "CargoScheduling",
      &format!(
        "Cargo {cargo_key:?} persisted an assignment for node {node_name:?} outside its placement result"
      ),
    ));
  }
  Ok(dispatches)
}

async fn reconcile_and_dispatch<R, RFut, D, DFut>(
  cargo_key: &str,
  selection: SelectionPlan,
  reconcile: R,
  mut dispatch: D,
) -> Result<Vec<CargoReplicaAssignment>, IoError>
where
  R: FnOnce(String, SelectionPlan) -> RFut,
  RFut: Future<Output = Result<Vec<CargoReplicaAssignment>, IoError>>,
  D: FnMut((NodeDb, Vec<CargoReplicaTask>)) -> DFut,
  DFut: Future<Output = Result<(), IoError>>,
{
  // The repository transaction commits before this future returns. Only the
  // returned persisted assignments are used to derive exact dispatch tasks.
  let assignments = reconcile(cargo_key.to_owned(), selection.clone()).await?;
  if assignments.len() != selection.total_replicas
    || assignments
      .iter()
      .any(|assignment| assignment.node_name.is_none())
  {
    return Err(IoError::other(
      "CargoScheduling",
      &format!(
        "Cargo {cargo_key:?} has {} assignments for {} desired replicas, including an unassigned replica",
        assignments.len(),
        selection.total_replicas
      ),
    ));
  }
  let dispatches =
    group_assigned_replicas_by_node(cargo_key, &assignments, &selection)?;
  for assignment in dispatches {
    dispatch(assignment).await?;
  }
  Ok(assignments)
}

pub(crate) fn http_error(error: IoError) -> HttpError {
  if error.inner.kind() == std::io::ErrorKind::WouldBlock {
    HttpError::conflict(error.to_string())
  } else {
    error.into()
  }
}

fn action_name(action: &NativeEventAction) -> HttpResult<&'static str> {
  match action {
    NativeEventAction::Starting => Ok("start"),
    NativeEventAction::Stopping => Ok("stop"),
    NativeEventAction::Restart => Ok("restart"),
    NativeEventAction::Updating => Ok("update"),
    NativeEventAction::Destroying => Ok("delete"),
    _ => Err(HttpError::bad_request("Unsupported Cargo operation")),
  }
}

fn validate_local_action<'a>(
  action: &str,
  local_node: &str,
  replica_nodes: impl IntoIterator<Item = Option<&'a str>>,
  process_nodes: impl IntoIterator<Item = &'a str>,
) -> HttpResult<()> {
  if matches!(action, "stop" | "restart")
    && (replica_nodes
      .into_iter()
      .flatten()
      .any(|node| node != local_node)
      || process_nodes.into_iter().any(|node| node != local_node))
  {
    return Err(HttpError::bad_request(
      "Cargo stop and restart require every assigned replica and process to be local to this node",
    ));
  }
  Ok(())
}

pub(crate) async fn accept_task<P, F>(
  key: &str,
  action: NativeEventAction,
  state: &SystemState,
  prepare: P,
) -> HttpResult<Cargo>
where
  P: FnOnce(OperationOwner, CargoOperationLease) -> F + 'static,
  F: Future<Output = HttpResult<AcceptedCargoOperation>> + 'static,
{
  let phase = action_name(&action)?;
  accept_owned(key, phase, action, state, prepare, None).await
}

async fn accept_owned<P, F>(
  key: &str,
  phase: &'static str,
  event: NativeEventAction,
  state: &SystemState,
  prepare: P,
  target: Option<(String, Option<String>)>,
) -> HttpResult<Cargo>
where
  P: FnOnce(OperationOwner, CargoOperationLease) -> F + 'static,
  F: Future<Output = HttpResult<AcceptedCargoOperation>> + 'static,
{
  let guard = utils::container::cargo::lifecycle_lock(key)
    .try_lock_owned()
    .map_err(|_| HttpError::conflict("A Cargo lifecycle action is active"))?;
  let key = key.to_owned();
  let state = state.clone();
  let (reply, response) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    state
      .inner
      .task_manager
      .wait_task(&format!("Cargo@{key}"))
      .await;
    let incarnation = match CargoDb::incarnation(&key, &state.inner.pool).await
    {
      Ok(value) => value,
      Err(error) => {
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    let mutex = match CargoDb::create_mutex(&key, phase, &state).await {
      Ok(value) => value,
      Err(error) => {
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    let (owner, owner_guard) = match utils::operation::create_owner(
      mutex.key,
      "Cargo",
      &key,
      incarnation,
      &state.inner.config.hostname,
      &state.inner.config.state_dir,
    ) {
      Ok(value) => value,
      Err(error) => {
        if let Err(cleanup) = CargoDb::delete_mutex(&mutex.key, &state).await {
          log::error!(
            "Cargo acceptance cleanup failed: {key} {}: {cleanup}",
            mutex.key
          );
        }
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    let lease = (std::sync::Arc::new(guard), owner_guard);
    let accepted = match prepare(owner, lease.clone()).await {
      Ok(value) => value,
      Err(error) => {
        match utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
        {
          Ok(true) => {
            if let Err(cleanup) =
              CargoDb::delete_mutex(&mutex.key, &state).await
            {
              log::error!(
                "Cargo acceptance cleanup failed: {key} {}: {cleanup}",
                mutex.key
              );
            }
          }
          Ok(false) => {}
          Err(cleanup) => log::warn!(
            "Cargo acceptance cleanup deferred: {key} {}: {cleanup}",
            mutex.key
          ),
        }
        let _ = reply.send(Err(error));
        return;
      }
    };
    let cargo = accepted.cargo.clone();
    let operation = accepted.operation.key;
    let context = CargoRuntimeContext {
      operation: accepted.operation.clone(),
      lease: (lease.0, Some(lease.1)),
      state: state.clone(),
    };
    let (start, ready) = tokio::sync::oneshot::channel();
    let (completed, completion) = tokio::sync::oneshot::channel();
    let task = owned_executor(async move {
      let _ = ready.await;
      let result = execute(accepted, context, target).await;
      let _ = completed.send(result);
      Ok(())
    });
    state
      .inner
      .task_manager
      .add_task(
        &format!("Cargo@{key}"),
        event.clone(),
        task,
        |_error| async { Ok(()) },
      )
      .await;
    if event != NativeEventAction::Restart {
      match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        publish_acceptance(&cargo, event, operation, &state),
      )
      .await
      {
        Ok(Ok(())) => {}
        _ => log::warn!(
          "Cargo operation {operation} acceptance notification unavailable; executing registered owner"
        ),
      }
    }
    let _ = start.send(());
    if is_targeted_action(phase) {
      match completion.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
          let _ = reply.send(Err(http_error(error)));
          return;
        }
        Err(_) => {
          let _ = reply.send(Err(HttpError::internal_server_error(
            "Cargo process owner stopped before reporting its result",
          )));
          return;
        }
      }
    }
    let _ = reply.send(Ok(cargo));
  });
  response.await.map_err(|_| {
    HttpError::internal_server_error(
      "Cargo acceptance owner stopped before replying",
    )
  })?
}

/// TaskManager owns a waiter; cancellation cannot release the execution lease.
fn owned_executor(
  future: impl Future<Output = IoResult<()>> + 'static,
) -> ObjTaskFuture {
  let (send, receive) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let _ = send.send(future.await);
  });
  Box::pin(async move {
    receive.await.map_err(|_| {
      IoError::interrupted("Cargo task", "Execution owner stopped unexpectedly")
    })?
  })
}

async fn publish_acceptance(
  cargo: &Cargo,
  action: NativeEventAction,
  operation: uuid::Uuid,
  state: &SystemState,
) -> IoResult<()> {
  let event: Event = EventDb::create_try_from(
    EventPartial {
      reporting_controller: crate::vars::CONTROLLER_NAME.to_owned(),
      reporting_node: state.inner.config.hostname.clone(),
      kind: EventKind::Normal,
      action: action.to_string(),
      reason: "state_sync".to_owned(),
      note: Some(format!("Cargo {}", cargo.spec.cargo_key)),
      actor: Some(cargo.clone().into()),
      related: None,
      metadata: Some(
        serde_json::json!({"Operation": operation, "SpecKey": cargo.spec.key}),
      ),
    },
    &state.inner.pool,
  )
  .await?
  .try_into()?;
  state.inner.event_emitter_raw.emit(&event)
}

async fn accept_current(
  key: &str,
  event: NativeEventAction,
  state: &SystemState,
) -> HttpResult<()> {
  let phase = action_name(&event)?;
  let prepare_key = key.to_owned();
  let prepare_state = state.clone();
  accept_task(key, event, state, move |owner, lease| async move {
    let cargo =
      CargoDb::transform_read_by_pk(&prepare_key, &prepare_state.inner.pool)
        .await?;
    // This runs with both the lifecycle and operation owner leases held, before
    // an operation or desired status is accepted. Only start/delete dispatch
    // work to other nodes; stop/restart operate on this node's Docker runtime.
    if matches!(phase, "stop" | "restart") {
      let replicas =
        CargoReplicaDb::list_by_cargo(&prepare_key, &prepare_state.inner.pool)
          .await?;
      let processes = ProcessDb::read_by_kind_key(
        &prepare_key,
        None,
        &prepare_state.inner.pool,
      )
      .await?;
      validate_local_action(
        phase,
        &prepare_state.inner.config.hostname,
        replicas.iter().map(|replica| replica.node_name.as_deref()),
        processes.iter().map(|process| process.node_name.as_str()),
      )?;
    }
    let operation =
      OperationDb::new(&owner, "Cargo", &prepare_key, cargo.spec.key, phase);
    CargoDb::accept_operation(
      &cargo,
      None,
      operation,
      lease,
      &prepare_state.inner.pool,
    )
    .await
    .map_err(http_error)
  })
  .await
  .map(|_| ())
}

pub(crate) async fn start(key: &str, state: &SystemState) -> HttpResult<()> {
  accept_current(key, NativeEventAction::Starting, state).await
}
pub(crate) async fn stop(key: &str, state: &SystemState) -> HttpResult<()> {
  accept_current(key, NativeEventAction::Stopping, state).await
}
pub(crate) async fn restart(key: &str, state: &SystemState) -> HttpResult<()> {
  accept_current(key, NativeEventAction::Restart, state).await
}
pub(crate) async fn refresh(
  key: &str,
  expected_spec: uuid::Uuid,
  state: &SystemState,
) -> HttpResult<()> {
  let prepare_key = key.to_owned();
  let prepare_state = state.clone();
  accept_task(
    key,
    NativeEventAction::Updating,
    state,
    move |owner, lease| async move {
      let cargo =
        CargoDb::transform_read_by_pk(&prepare_key, &prepare_state.inner.pool)
          .await?;
      if cargo.spec.key != expected_spec {
        return Err(HttpError::conflict(
          "Cargo revision changed before secret refresh",
        ));
      }
      let operation = OperationDb::new(
        &owner,
        "Cargo",
        &prepare_key,
        expected_spec,
        "update",
      );
      CargoDb::accept_operation(
        &cargo,
        None,
        operation,
        lease,
        &prepare_state.inner.pool,
      )
      .await
      .map_err(http_error)
    },
  )
  .await
  .map(|_| ())
}

pub(crate) async fn process_action(
  key: &str,
  id: &str,
  action: &str,
  signal: Option<&str>,
  state: &SystemState,
) -> HttpResult<()> {
  let phase = match action {
    "stop_process" => "stop_process",
    "restart_process" => "restart_process",
    "signal" => "signal",
    _ => {
      return Err(HttpError::bad_request("Unsupported Cargo process action"));
    }
  };
  let prepare_key = key.to_owned();
  let prepare_id = id.to_owned();
  let prepare_state = state.clone();
  accept_owned(
    key,
    phase,
    NativeEventAction::Restart,
    state,
    move |owner, lease| async move {
      let cargo =
        CargoDb::transform_read_by_pk(&prepare_key, &prepare_state.inner.pool)
          .await?;
      utils::container::cargo::validate_target(
        &cargo,
        owner.incarnation_key,
        &prepare_id,
        &prepare_state,
      )
      .await?;
      let operation =
        OperationDb::new(&owner, "Cargo", &prepare_key, cargo.spec.key, phase);
      CargoDb::accept_operation(
        &cargo,
        None,
        operation,
        lease,
        &prepare_state.inner.pool,
      )
      .await
      .map_err(http_error)
    },
    Some((id.to_owned(), signal.map(str::to_owned))),
  )
  .await
  .map(|_| ())
}

async fn dispatch_start(
  cargo: Cargo,
  assignment: (NodeDb, Vec<CargoReplicaTask>),
  context: CargoRuntimeContext,
) -> IoResult<()> {
  let (node, replicas) = assignment;
  utils::container::cargo::ensure_current(&context).await?;
  if node.name == context.inner.config.hostname
    || node.name == "init-test.nanocl.io"
  {
    return utils::container::cargo::start(&cargo, &replicas, &context).await;
  }
  let client =
    nanocld_client::NanocldClient::connect_to(&nanocld_client::ConnectOpts {
      url: node.endpoint,
      ssl: None,
      version: Some(node.version),
    })?;
  client
    .start_node_cargo_operation(
      &nanocl_stubs::node::StartNodeCargoOperationParams {
        cargo_key: cargo.spec.cargo_key.clone(),
        operation_key: context.operation.key,
        incarnation_key: context.operation.incarnation_key,
        spec_key: context.operation.spec_key,
        replicas,
      },
    )
    .await
    .map_err(|_| {
      IoError::interrupted(
        "Cargo remote execution",
        "Node did not confirm completion of the accepted Cargo operation",
      )
    })?;
  Ok(())
}

async fn execute_start(
  cargo: &Cargo,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  let key = &cargo.spec.cargo_key;
  let selection =
    utils::container::generic::plan_selection(cargo, state).await?;
  let reconcile_state = state.clone();
  let dispatch_state = state.clone();
  let dispatch_cargo = cargo.clone();
  reconcile_and_dispatch(
    key,
    selection,
    move |key, selection| async move {
      CargoReplicaDb::reconcile_assignments_owned(
        &key,
        &selection,
        Some(reconcile_state.operation.owner()),
        reconcile_state.lease.clone(),
        &reconcile_state.inner.pool,
      )
      .await
    },
    move |assignment| {
      dispatch_start(dispatch_cargo.clone(), assignment, dispatch_state.clone())
    },
  )
  .await?;
  let processes =
    ProcessDb::read_by_kind_key(key, None, &state.inner.pool).await?;
  if utils::container::cargo::has_container_healthcheck(cargo, &processes) {
    ObjPsStatusDb::update_owned(
      key,
      ObjPsStatusUpdate {
        health: Some(ObjPsHealthStatusKind::Healthy.to_string()),
        ..Default::default()
      },
      Some(state.operation.owner()),
      state.lease.clone(),
      &state.inner.pool,
    )
    .await?;
  }
  ObjPsStatusDb::update_actual_status_owned(
    key,
    &ObjPsStatusKind::Start,
    Some(state.operation.owner()),
    state.lease.clone(),
    &state.inner.pool,
  )
  .await?;
  for action in utils::container::cargo::promotion_actions(
    utils::container::cargo::has_container_healthcheck(cargo, &processes),
  ) {
    state.emit_normal_native_action_sync(cargo, action).await;
  }
  Ok(())
}

fn completion_action(action: &str) -> NativeEventAction {
  match action {
    "start" => NativeEventAction::Start,
    "stop" | "stop_process" => NativeEventAction::Stop,
    "update" => NativeEventAction::Update,
    "delete" => NativeEventAction::Destroy,
    _ => NativeEventAction::Restart,
  }
}

fn uncertain_runtime_error(error: &IoError) -> bool {
  matches!(
    error.context(),
    Some(
      "CreateProcess"
        | "DeleteProcess"
        | "StartCargoProcess"
        | "StopCargoProcess"
        | "RestartCargoProcess"
        | "RenameCargoProcess"
        | "MarkCargoCandidate"
        | "InspectCargoProcess"
        | "CargoInitContainer"
        | "Cargo remote execution"
        | "Cargo remote observation"
        | "Cargo process restart"
        | "Cargo process signal"
        | "Cargo rollback"
    )
  )
}

async fn execute(
  accepted: AcceptedCargoOperation,
  state: CargoRuntimeContext,
  target: Option<(String, Option<String>)>,
) -> IoResult<()> {
  let action = accepted.operation.action.as_str();
  let result = match utils::container::cargo::ensure_current(&state).await {
    Err(error) => Err(error),
    Ok(()) => match action {
      "start" => execute_start(&accepted.cargo, &state).await,
      "restart" => {
        utils::container::cargo::restart(&accepted.cargo, &state).await
      }
      "update" => {
        utils::container::cargo::update(&accepted.cargo, &state).await
      }
      "stop" => utils::container::cargo::stop(&accepted.cargo, &state).await,
      "delete" => {
        utils::container::cargo::delete(
          &accepted.operation.resource_key,
          &state,
        )
        .await
      }
      _ => match target {
        Some((id, signal)) => {
          utils::container::cargo::process_action(
            &accepted.cargo,
            &id,
            signal.as_deref(),
            &state,
          )
          .await
        }
        None => Err(IoError::invalid_input(
          "Cargo operation",
          "Missing targeted process",
        )),
      },
    },
  };
  let mut reported = result.as_ref().map(|_| ()).map_err(|error| {
    IoError::with_context(
      "Cargo operation",
      std::io::Error::new(
        error.inner.kind(),
        "Accepted Cargo operation did not complete",
      ),
    )
  });
  match result {
    Ok(()) => {
      let finished = if action == "delete" {
        CargoDb::delete_operation(
          &accepted.operation,
          state.lease.clone(),
          &state.inner.pool,
        )
        .await
      } else {
        CargoDb::complete_operation(
          &accepted.operation,
          "completed",
          None,
          state.lease.clone(),
          &state.inner.pool,
        )
        .await
      };
      let settled = matches!(&finished, Ok(true));
      match finished {
        Ok(true) => {
          if action != "start" && !is_targeted_action(action) {
            state
              .emit_normal_native_action_sync(
                &accepted.cargo,
                completion_action(action),
              )
              .await;
          }
        }
        Ok(false) => {
          log::warn!(
            "Cargo operation {} lost completion ownership",
            accepted.operation.key
          );
          reported = Err(IoError::interrupted(
            "Cargo operation",
            "Operation completion ownership was lost",
          ));
        }
        Err(error) => {
          log::warn!(
            "Cargo operation {} completion remains pending: {error}",
            accepted.operation.key
          );
          reported = Err(IoError::interrupted(
            "Cargo operation",
            "Operation completion remains pending",
          ));
        }
      }
      if !settled
        && !is_targeted_action(action)
        && CargoDb::validate_operation(
          &accepted.operation,
          state.lease.clone(),
          &state.inner.pool,
        )
        .await
        .unwrap_or(false)
      {
        state.emit_error_native_action(&accepted.cargo, completion_action(action), Some(format!("Cargo operation {} ({action}) runtime work finished but completion could not be recorded; the operation remains pending", accepted.operation.key)));
      }
    }
    Err(error) => {
      let owns_failure = CargoDb::validate_operation(
        &accepted.operation,
        state.lease.clone(),
        &state.inner.pool,
      )
      .await
      .unwrap_or(false);
      let remote = CargoReplicaDb::list_by_cargo(
        &accepted.operation.resource_key,
        &state.inner.pool,
      )
      .await
      .map(|replicas| {
        replicas.iter().any(|replica| {
          replica.node_name.as_deref().is_some_and(|node| {
            node != state.inner.config.hostname && node != "init-test.nanocl.io"
          })
        })
      })
      .unwrap_or(true);
      if owns_failure
        && (!remote || is_targeted_action(action))
        && !uncertain_runtime_error(&error)
      {
        let update = if is_targeted_action(action) {
          ObjPsStatusUpdate::default()
        } else {
          ObjPsStatusUpdate {
            actual: Some(ObjPsStatusKind::Fail.to_string()),
            prev_actual: if action == "update" {
              Some(ObjPsStatusKind::Updating.to_string())
            } else {
              None
            },
            health: Some(ObjPsHealthStatusKind::Unhealthy.to_string()),
            ..Default::default()
          }
        };
        // Init exit diagnostics contain only the confirmed numeric exit code,
        // not arbitrary Docker error text or container output.
        let failure = if error.context() == Some("CargoInitContainerExit") {
          error.inner.to_string()
        } else {
          "Accepted Cargo operation failed; inspect the observed runtime before retrying".to_owned()
        };
        let diagnostic =
          serde_json::json!({"Phase": action, "Failure": failure});
        if let Err(error) = CargoDb::finish_operation(
          &accepted.operation,
          update,
          "failed",
          Some(diagnostic),
          state.lease.clone(),
          &state.inner.pool,
        )
        .await
        {
          log::warn!(
            "Cargo operation {} failure settlement deferred: {error}",
            accepted.operation.key
          );
        }
      }
      log::warn!(
        "Cargo operation {} ({action}) failed in {:?}",
        accepted.operation.key,
        error.context()
      );
      if owns_failure && !is_targeted_action(action) {
        state.emit_error_native_action(&accepted.cargo, completion_action(action), Some(format!("Cargo operation {} ({action}) could not complete; inspect its runtime and pending operation before retrying", accepted.operation.key)));
      }
    }
  }
  if let Some(owner) = &state.lease.1
    && let Err(error) =
      utils::operation::cleanup_owner(owner, &state.inner.pool).await
  {
    log::warn!("Cargo operation owner cleanup deferred: {error}");
  }
  reported
}

fn is_targeted_action(action: &str) -> bool {
  matches!(action, "stop_process" | "restart_process" | "signal")
}

/// The receiver owns delegated work independently from the HTTP request.
/// Only the coordinator settles the shared operation after node acknowledgments.
pub(crate) async fn run_node_start(
  operation: OperationDb,
  replicas: Vec<CargoReplicaTask>,
  state: &SystemState,
) -> IoResult<()> {
  run_node(operation, Some(replicas), state).await
}
pub(crate) async fn run_node_delete(
  operation: OperationDb,
  state: &SystemState,
) -> IoResult<()> {
  run_node(operation, None, state).await
}
async fn run_node(
  operation: OperationDb,
  replicas: Option<Vec<CargoReplicaTask>>,
  state: &SystemState,
) -> IoResult<()> {
  let guard = utils::container::cargo::lifecycle_lock(&operation.resource_key)
    .try_lock_owned()
    .map_err(|_| {
      IoError::with_context(
        "Cargo delegated operation",
        std::io::Error::new(
          std::io::ErrorKind::WouldBlock,
          "A local Cargo worker is active",
        ),
      )
    })?;
  let lease = (std::sync::Arc::new(guard), None);
  let state = CargoRuntimeContext {
    operation,
    lease,
    state: state.clone(),
  };
  let (reply, response) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let result = async {
      utils::container::cargo::ensure_current(&state).await?;
      let cargo = CargoDb::transform_read_by_pk(
        &state.operation.resource_key,
        &state.inner.pool,
      )
      .await?;
      if let Some(mut requested) = replicas {
        if state.operation.action != "start" {
          return Err(IoError::invalid_input(
            "Cargo delegated operation",
            "Operation is not a start",
          ));
        }
        let mut assigned = CargoReplicaDb::list_by_cargo(
          &state.operation.resource_key,
          &state.inner.pool,
        )
        .await?
        .into_iter()
        .filter(|replica| {
          replica.node_name.as_deref()
            == Some(state.inner.config.hostname.as_str())
        })
        .map(|replica| CargoReplicaTask {
          key: replica.key,
          ordinal: replica.ordinal,
        })
        .collect::<Vec<_>>();
        assigned.sort_by_key(|replica| (replica.ordinal, replica.key));
        requested.sort_by_key(|replica| (replica.ordinal, replica.key));
        if requested != assigned {
          return Err(IoError::invalid_input(
            "Cargo delegated operation",
            "Requested replicas do not equal this node's persisted assignments",
          ));
        }
        utils::container::cargo::start(&cargo, &requested, &state).await
      } else {
        if state.operation.action != "delete" {
          return Err(IoError::invalid_input(
            "Cargo delegated operation",
            "Operation is not a delete",
          ));
        }
        utils::container::cargo::delete_instances(
          &state.operation.resource_key,
          false,
          &state,
        )
        .await
      }
    }
    .await;
    let _ = reply.send(result);
  });
  response.await.map_err(|_| {
    IoError::interrupted(
      "Cargo delegated operation",
      "Delegated worker stopped before reporting its result",
    )
  })?
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use crate::utils::container::generic::SelectionAssignment;
  use crate::vars;

  use super::*;

  #[ntex::test]
  async fn cancelled_cargo_waiter_retains_executor_ownership_until_completion()
  {
    let key = format!("global.cargo-waiter-{}", uuid::Uuid::new_v4());
    let lifecycle = utils::container::cargo::lifecycle_lock(&key);
    let guard = lifecycle.clone().try_lock_owned().unwrap();
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, proceed) = tokio::sync::oneshot::channel();
    let (finished, completed) = tokio::sync::oneshot::channel();
    let task = owned_executor(async move {
      entered.send(()).unwrap();
      proceed.await.unwrap();
      drop(guard);
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
    assert!(lifecycle.clone().try_lock_owned().is_err());
    release.send(()).unwrap();
    completed.await.unwrap();
    assert!(lifecycle.try_lock_owned().is_ok());
  }

  #[test]
  fn cargo_targeted_actions_do_not_claim_whole_cargo_completion() {
    for action in ["stop_process", "restart_process", "signal"] {
      assert!(is_targeted_action(action));
    }
    for action in ["start", "stop", "restart", "update", "delete"] {
      assert!(!is_targeted_action(action));
    }
    assert!(uncertain_runtime_error(&IoError::interrupted(
      "Cargo remote execution",
      "unconfirmed"
    )));
    assert!(uncertain_runtime_error(&IoError::interrupted(
      "Cargo process signal",
      "unconfirmed"
    )));
    assert!(uncertain_runtime_error(&IoError::interrupted(
      "Cargo rollback",
      "unconfirmed restoration"
    )));
    assert!(!uncertain_runtime_error(&IoError::invalid_input(
      "Cargo process rejection",
      "rejected"
    )));
  }

  #[test]
  fn confirmed_init_exit_can_settle_but_unknown_wait_remains_pending() {
    assert!(!uncertain_runtime_error(&IoError::interrupted(
      "CargoInitContainerExit",
      "Init container exited with code 17"
    )));
    for context in ["CargoInitContainer", "Cargo remote execution"] {
      assert!(uncertain_runtime_error(&IoError::interrupted(
        context,
        "outcome is not established"
      )));
    }
  }

  #[test]
  fn cargo_ownership_conflicts_return_http_conflict() {
    let error = IoError::with_context(
      "Cargo ownership",
      std::io::Error::new(std::io::ErrorKind::WouldBlock, "stale operation"),
    );
    assert_eq!(http_error(error).status, ntex::http::StatusCode::CONFLICT);
    assert_eq!(
      http_error(IoError::invalid_input("Cargo", "invalid request")).status,
      ntex::http::StatusCode::BAD_REQUEST,
    );
  }

  #[test]
  fn cargo_local_actions_reject_remote_replicas_and_processes_before_admission()
  {
    for action in ["stop", "restart"] {
      assert!(
        validate_local_action(
          action,
          "node-a",
          [Some("node-a"), None],
          ["node-a"]
        )
        .is_ok()
      );
      assert!(validate_local_action(action, "node-a", [], []).is_ok());
      for (replicas, processes) in [
        (vec![Some("node-b")], vec!["node-a"]),
        (vec![Some("node-a")], vec!["node-b"]),
        (vec![], vec!["node-b"]),
        (vec![Some("init-test.nanocl.io")], vec![]),
      ] {
        let error =
          validate_local_action(action, "node-a", replicas, processes)
            .unwrap_err();
        assert_eq!(error.status, ntex::http::StatusCode::BAD_REQUEST);
      }
    }
    for action in ["start", "delete"] {
      assert!(
        validate_local_action(action, "node-a", [Some("node-b")], ["node-b"])
          .is_ok()
      );
    }
  }

  fn node(name: &str) -> NodeDb {
    NodeDb {
      name: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      endpoint: format!("tcp://{name}:8585"),
      version: vars::VERSION.to_owned(),
      metadata: None,
    }
  }

  fn selection(
    total_replicas: usize,
    assignments: &[(&str, usize)],
  ) -> SelectionPlan {
    SelectionPlan {
      total_replicas,
      assignments: assignments
        .iter()
        .map(|(node_name, replicas)| SelectionAssignment {
          node: node(node_name),
          replicas: *replicas,
        })
        .collect(),
    }
  }

  fn assignment(
    ordinal: i32,
    node_name: Option<&str>,
  ) -> CargoReplicaAssignment {
    CargoReplicaAssignment {
      replica_key: uuid::Uuid::new_v4(),
      ordinal,
      node_name: node_name.map(str::to_owned),
    }
  }

  #[test]
  fn assigned_replica_grouping_preserves_stable_identity_and_order() {
    let selection = selection(4, &[("node-b", 1), ("node-a", 2)]);
    // Deliberately use noncontiguous, unsorted node-local ordinals and differ
    // from the planner quotas. Only committed durable assignments may be
    // dispatched.
    let assignments = vec![
      assignment(7, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(4, Some("node-a")),
      assignment(2, None),
    ];

    let grouped = group_assigned_replicas_by_node(
      "global.scheduler",
      &assignments,
      &selection,
    )
    .unwrap();
    assert_eq!(grouped[0].0.name, "node-b");
    assert_eq!(
      grouped[0].1,
      [CargoReplicaTask {
        key: assignments[1].replica_key,
        ordinal: 1,
      }]
    );
    assert_eq!(grouped[1].0.name, "node-a");
    assert_eq!(
      grouped[1].1,
      [
        CargoReplicaTask {
          key: assignments[2].replica_key,
          ordinal: 4,
        },
        CargoReplicaTask {
          key: assignments[0].replica_key,
          ordinal: 7,
        },
      ]
    );
    assert_eq!(
      grouped
        .iter()
        .map(|(_, replicas)| replicas.len())
        .sum::<usize>(),
      3,
      "the unassigned desired replica must not be dispatched"
    );
  }

  #[ntex::test]
  async fn scheduling_reconciles_before_dispatching_persisted_replicas() {
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured =
      Arc::new(Mutex::new(Vec::<(String, Vec<(uuid::Uuid, i32)>)>::new()));
    let reconcile_events = events.clone();
    let dispatch_events = events.clone();
    let dispatch_captured = captured.clone();
    let persisted =
      vec![assignment(0, Some("node-a")), assignment(1, Some("node-b"))];
    let returned = persisted.clone();

    let result = reconcile_and_dispatch(
      "global.scheduler",
      selection(2, &[("node-b", 1), ("node-a", 1)]),
      move |cargo_key, selection| {
        let events = reconcile_events.clone();
        let assignments = returned.clone();
        async move {
          assert_eq!(cargo_key, "global.scheduler");
          assert_eq!(selection.total_replicas, 2);
          assert_eq!(
            selection
              .assignments
              .iter()
              .map(|assignment| {
                (assignment.node.name.as_str(), assignment.replicas)
              })
              .collect::<Vec<_>>(),
            [("node-b", 1), ("node-a", 1)]
          );
          events.lock().unwrap().push("reconcile".to_owned());
          Ok(assignments)
        }
      },
      move |(node, replicas)| {
        let events = dispatch_events.clone();
        let captured = dispatch_captured.clone();
        async move {
          events
            .lock()
            .unwrap()
            .push(format!("dispatch:{}", node.name));
          captured.lock().unwrap().push((
            node.name,
            replicas
              .into_iter()
              .map(|replica| (replica.key, replica.ordinal))
              .collect(),
          ));
          Ok(())
        }
      },
    )
    .await
    .unwrap();

    assert_eq!(result, persisted);
    assert_eq!(
      *events.lock().unwrap(),
      ["reconcile", "dispatch:node-b", "dispatch:node-a"]
    );
    assert_eq!(
      *captured.lock().unwrap(),
      [
        (
          "node-b".to_owned(),
          vec![(persisted[1].replica_key, persisted[1].ordinal)],
        ),
        (
          "node-a".to_owned(),
          vec![(persisted[0].replica_key, persisted[0].ordinal)],
        ),
      ]
    );
  }

  #[ntex::test]
  async fn incomplete_assignment_prevents_every_dispatch() {
    let dispatches = Arc::new(Mutex::new(0usize));
    let observed_dispatches = dispatches.clone();
    let persisted = vec![
      assignment(0, Some("node-a")),
      assignment(1, Some("node-b")),
      assignment(2, None),
    ];
    let error = reconcile_and_dispatch(
      "global.scheduler",
      selection(3, &[("node-a", 1), ("node-b", 1)]),
      move |_, _| {
        let persisted = persisted.clone();
        async move { Ok(persisted) }
      },
      move |_| {
        let dispatches = observed_dispatches.clone();
        async move {
          *dispatches.lock().unwrap() += 1;
          Ok(())
        }
      },
    )
    .await
    .unwrap_err();

    assert_eq!(error.context(), Some("CargoScheduling"));
    assert_eq!(*dispatches.lock().unwrap(), 0);
  }

  #[ntex::test]
  async fn scheduling_repository_failure_prevents_dispatch() {
    let dispatches = Arc::new(Mutex::new(0usize));
    let observed_dispatches = dispatches.clone();
    let error = reconcile_and_dispatch(
      "global.scheduler",
      selection(1, &[("node-a", 1)]),
      |_, _| async {
        Err(IoError::other(
          "CargoReplicaReconcile",
          "injected repository failure",
        ))
      },
      move |_| {
        let dispatches = observed_dispatches.clone();
        async move {
          *dispatches.lock().unwrap() += 1;
          Ok(())
        }
      },
    )
    .await
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(*dispatches.lock().unwrap(), 0);
  }

  #[test]
  fn assigned_replica_grouping_rejects_unknown_persisted_nodes() {
    let error = group_assigned_replicas_by_node(
      "global.scheduler",
      &[assignment(0, Some("node-b"))],
      &selection(1, &[("node-a", 1)]),
    )
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert_eq!(error.context(), Some("CargoScheduling"));
    assert!(error.to_string().contains("node-b"));
  }
}
