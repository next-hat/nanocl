use std::{
  path::PathBuf,
  sync::{Arc, OnceLock},
  time::Duration,
};

use bollard_next::{
  models::ContainerInspectResponse,
  query_parameters::{
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
  },
};
use futures::StreamExt;
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
  vm::{Vm, VmLifecycleOptions, VmRuntimeStatus},
};
use tokio::time::{Instant, timeout_at};

use crate::{
  models::{
    ObjPsStatusDb, ObjPsStatusUpdate, OperationDb, OperationOwner, ProcessDb,
    QmpClient, SystemState, VmDb, VmLifecycleGuard, VmLifecycleLock,
    VmLifecycleLocks, VmLifecycleObservationGuard, VmOperationLease,
    VmRestartTask,
  },
  repositories::generic::*,
};

pub(crate) fn lifecycle_lock(key: &str) -> Arc<VmLifecycleLock> {
  static LOCKS: OnceLock<VmLifecycleLocks> = OnceLock::new();
  let mut locks = LOCKS
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|err| err.into_inner());
  locks.retain(|_, lock| lock.strong_count() > 0);
  if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
    return lock;
  }
  let lock = Arc::new(VmLifecycleLock {
    key: key.to_owned(),
    ..Default::default()
  });
  locks.insert(key.to_owned(), Arc::downgrade(&lock));
  lock
}

impl VmLifecycleLock {
  /// Already accepted tasks wait for the preceding mutation and observation.
  pub(crate) async fn lock_owned(self: Arc<Self>) -> VmLifecycleGuard {
    let mutation = self.mutation.clone().lock_owned().await;
    let observation = self.observation.clone().lock_owned().await;
    VmLifecycleGuard {
      _observation: observation,
      _mutation: mutation,
      _lock: self,
    }
  }

  /// Reject concurrent mutations, but let an in-flight observation finish its
  /// protected status write before starting this operation. No work is accepted
  /// if observation completion exceeds the bounded admission wait.
  pub(crate) async fn try_lock_owned(
    self: Arc<Self>,
  ) -> IoResult<VmLifecycleGuard> {
    self
      .try_lock_owned_before(Instant::now() + Duration::from_secs(30))
      .await
  }

  async fn try_lock_owned_before(
    self: Arc<Self>,
    deadline: Instant,
  ) -> IoResult<VmLifecycleGuard> {
    let mutation = self.mutation.clone().try_lock_owned().map_err(|_| {
      log::debug!(
        "VM {} admission rejected: another mutation owns the lifecycle",
        self.key
      );
      conflict("A VM lifecycle mutation is active")
    })?;
    let observation = match self.observation.clone().try_lock_owned() {
      Ok(guard) => guard,
      Err(_) => {
        log::debug!(
          "VM {} admission waits for Docker observation completion",
          self.key
        );
        timeout_at(
          deadline,
          self.observation.clone().lock_owned(),
        )
        .await
        .map_err(|_| {
          log::warn!("VM {} admission timed out waiting for Docker observation; no mutation started", self.key);
          IoError::with_context(
            "VM lifecycle observation",
            std::io::Error::new(
              std::io::ErrorKind::TimedOut,
              "Docker runtime observation did not finish within 30 seconds; no VM mutation was started",
            ),
          )
        })?
      }
    };
    Ok(VmLifecycleGuard {
      _observation: observation,
      _mutation: mutation,
      _lock: self,
    })
  }

  pub(crate) fn try_observe_owned(
    self: Arc<Self>,
  ) -> Option<VmLifecycleObservationGuard> {
    let observation = self.observation.clone().try_lock_owned().ok()?;
    Some(VmLifecycleObservationGuard {
      _observation: observation,
      _lock: self,
    })
  }

  pub(crate) async fn observe_owned(
    self: Arc<Self>,
  ) -> VmLifecycleObservationGuard {
    let observation = self.observation.clone().lock_owned().await;
    VmLifecycleObservationGuard {
      _observation: observation,
      _lock: self,
    }
  }
}

fn conflict(message: &str) -> IoError {
  IoError::with_context(
    "VM lifecycle",
    std::io::Error::new(std::io::ErrorKind::WouldBlock, message),
  )
}

fn shutdown_timeout() -> IoError {
  IoError::with_context(
    "VM shutdown",
    std::io::Error::new(
      std::io::ErrorKind::TimedOut,
      "Guest shutdown was not confirmed before the deadline; no forced power-off was performed. Inspect the VM and use an explicit force-stop if needed",
    ),
  )
}

fn is_running(runtime: &ContainerInspectResponse) -> IoResult<bool> {
  runtime
    .state
    .as_ref()
    .and_then(|state| state.running)
    .ok_or_else(|| conflict("VM runtime running state is unknown"))
}

fn validate_process_owner(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  processes: &[Process],
  node: &str,
) -> IoResult<()> {
  let id = super::owned_runtime_id(vm, runtime)?;
  let name = format!("{}.v", vm.spec.vm_key);
  // Also permit recovery of a local Docker container whose process-row write
  // failed. If a row exists, it must identify this exact local container.
  for process in processes.iter().filter(|process| process.name == name) {
    if process.kind != ProcessKind::Vm
      || process.kind_key != vm.spec.vm_key
      || process.node_name != node
      || process.key != id
    {
      return Err(conflict(
        "VM runtime process identity does not match the local container",
      ));
    }
  }
  Ok(())
}

pub(crate) async fn inspect_runtime(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<Option<ContainerInspectResponse>> {
  let runtime = match state
    .inner
    .docker_api
    .inspect_container(&format!("{}.v", vm.spec.vm_key), None)
    .await
  {
    Ok(runtime) => runtime,
    Err(bollard_next::errors::Error::DockerResponseServerError {
      status_code: 404,
      ..
    }) => return Ok(None),
    Err(err) => return Err(*err.map_err_context(|| "VM runtime")),
  };
  let processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  validate_process_owner(
    vm,
    &runtime,
    &processes,
    &state.inner.config.hostname,
  )?;
  super::validate_runtime_generation(vm, &runtime, state).await?;
  Ok(Some(runtime))
}

pub(crate) fn socket_directory(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  state_dir: &str,
) -> IoResult<PathBuf> {
  super::owned_runtime_id(vm, runtime)?;
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  let label = |key: &str| {
    labels
      .and_then(|labels| labels.get(key))
      .map(String::as_str)
  };
  if label("io.nanocl.vm-qmp") != Some("1") {
    return Err(conflict(
      "VM runtime does not provide the required managed QMP interface",
    ));
  }
  let generation = label("io.nanocl.vm-spec")
    .and_then(|value| uuid::Uuid::parse_str(value).ok())
    .ok_or_else(|| conflict("VM runtime generation is missing or invalid"))?;
  // A desired update can already have a new spec while the old VM is still
  // running. Its own verified generation determines which socket to control.
  let key = vm
    .spec
    .vm_key
    .parse::<nanocl_stubs::resource_key::ResourceKey>()
    .map_err(|_| conflict("Invalid VM resource key"))?;
  let directory = std::fs::canonicalize(state_dir)?
    .join("run/vms")
    .join(key.to_string())
    .join(generation.to_string());
  let mounts = runtime.mounts.as_deref().unwrap_or_default();
  let mut matches = mounts
    .iter()
    .filter(|mount| mount.destination.as_deref() == Some("/run/nanocl-vm"));
  let mount = matches
    .next()
    .ok_or_else(|| conflict("VM runtime socket directory is not mounted"))?;
  if matches.next().is_some()
    || mount.source.as_deref() != directory.to_str()
    || mount.rw != Some(true)
    || mount.typ.as_deref() != Some("bind")
  {
    return Err(conflict(
      "VM runtime socket mount does not match its owner and generation",
    ));
  }
  Ok(directory)
}

pub(super) async fn connect(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  state: &SystemState,
  deadline: Instant,
) -> IoResult<QmpClient> {
  let directory = socket_directory(vm, runtime, &state.inner.config.state_dir)?;
  let (_socket_inode, socket) =
    super::open_runtime_socket(&directory, "qmp.sock")?;
  QmpClient::connect(&socket, deadline).await
}

/// Only a fresh Docker inspection of this immutable ID proves runtime exit.
async fn stopped(id: &str, state: &SystemState) -> IoResult<bool> {
  let runtime = state
    .inner
    .docker_api
    .inspect_container(id, None)
    .await
    .map_err(|err| err.map_err_context(|| "VM shutdown confirmation"))?;
  if runtime.id.as_deref() != Some(id) {
    return Err(conflict("VM runtime identity changed during shutdown"));
  }
  Ok(runtime.state.as_ref().and_then(|state| state.running) == Some(false))
}

pub(super) async fn stop_runtime(
  vm: &Vm,
  options: &VmLifecycleOptions,
  state: &SystemState,
) -> IoResult<Option<String>> {
  options
    .validate()
    .map_err(|err| IoError::invalid_input("VM shutdown", &err))?;
  let deadline = Instant::now() + Duration::from_secs(options.timeout);
  timeout_at(deadline, stop_runtime_before(vm, options, state, deadline))
    .await
    .map_err(|_| {
      if options.force {
        IoError::with_context(
          "VM force-stop",
          std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Forced runtime exit was not confirmed before the deadline",
          ),
        )
      } else {
        shutdown_timeout()
      }
    })?
}

async fn stop_runtime_before(
  vm: &Vm,
  options: &VmLifecycleOptions,
  state: &SystemState,
  deadline: Instant,
) -> IoResult<Option<String>> {
  let Some(runtime) = inspect_runtime(vm, state).await? else {
    return Ok(None);
  };
  let id = super::owned_runtime_id(vm, &runtime)?;
  if !is_running(&runtime)? {
    return Ok(Some(id));
  }
  if options.force {
    state
      .inner
      .docker_api
      .stop_container(
        &id,
        Some(StopContainerOptions {
          t: Some(0),
          ..Default::default()
        }),
      )
      .await
      .map_err(|err| err.map_err_context(|| "VM force-stop"))?;
  } else {
    let mut qmp = connect(vm, &runtime, state, deadline).await?;
    if let Err(err) = qmp.powerdown().await {
      // QEMU may exit before its response reaches the client. EOF alone is
      // insufficient, but observing this exact container stopped is decisive.
      if stopped(&id, state).await? {
        return Ok(Some(id));
      }
      return Err(err);
    }
  }
  let mut wait = state.inner.docker_api.wait_container(
    &id,
    Some(WaitContainerOptions {
      condition: "not-running".to_owned(),
    }),
  );
  // The enclosing stop_runtime deadline covers both the wait and inspection.
  let result = wait.next().await;
  if stopped(&id, state).await? {
    return Ok(Some(id));
  }
  match result {
    Some(Err(err)) => Err(*err.map_err_context(|| "VM shutdown confirmation")),
    _ => Err(IoError::interrupted(
      "VM shutdown",
      "Runtime exit was not confirmed",
    )),
  }
}

pub(crate) async fn runtime_status(
  key: &str,
  state: &SystemState,
) -> IoResult<VmRuntimeStatus> {
  // Read-only observations remain available during a shutdown grace period.
  // Identity and the generation-specific mount are checked on every query.
  let deadline = Instant::now() + Duration::from_secs(5);
  timeout_at(deadline, runtime_status_before(key, state, deadline))
    .await
    .map_err(|_| {
      IoError::with_context(
        "VM status",
        std::io::Error::new(
          std::io::ErrorKind::TimedOut,
          "VM status query timed out",
        ),
      )
    })?
}

async fn runtime_status_before(
  key: &str,
  state: &SystemState,
  deadline: Instant,
) -> IoResult<VmRuntimeStatus> {
  let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let Some(runtime) = inspect_runtime(&vm, state).await? else {
    return Ok(VmRuntimeStatus {
      running: false,
      status: "exited".into(),
    });
  };
  if !is_running(&runtime)? {
    return Ok(VmRuntimeStatus {
      running: false,
      status: "exited".into(),
    });
  }
  let mut qmp = connect(&vm, &runtime, state, deadline).await?;
  qmp.query_status().await
}

async fn set_status<G: Send + 'static>(
  key: &str,
  wanted: ObjPsStatusKind,
  actual: ObjPsStatusKind,
  owner: Option<OperationOwner>,
  guard: G,
  state: &SystemState,
) -> IoResult<()> {
  let old = ObjPsStatusDb::read_by_pk(key, &state.inner.pool).await?;
  ObjPsStatusDb::update_owned(
    key,
    ObjPsStatusUpdate {
      wanted: Some(wanted.to_string()),
      prev_wanted: Some(old.wanted),
      actual: Some(actual.to_string()),
      prev_actual: Some(old.actual),
      ..Default::default()
    },
    owner,
    guard,
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

async fn restore_observed_status<G: Send + 'static>(
  vm: &Vm,
  guard: G,
  state: &SystemState,
) -> IoResult<()> {
  let observed = timeout_at(
    Instant::now() + Duration::from_secs(5),
    inspect_runtime(vm, state),
  )
  .await;
  let actual = match observed {
    Ok(Ok(None)) => ObjPsStatusKind::Stop,
    Ok(Ok(Some(runtime))) => match is_running(&runtime) {
      Ok(true) => ObjPsStatusKind::Start,
      Ok(false) => ObjPsStatusKind::Stop,
      Err(_) => ObjPsStatusKind::Fail,
    },
    _ => ObjPsStatusKind::Fail,
  };
  // Never leave an unsuccessful stop queued for an automatic retry.
  let wanted = if actual == ObjPsStatusKind::Stop {
    ObjPsStatusKind::Stop
  } else {
    ObjPsStatusKind::Start
  };
  set_status(&vm.spec.vm_key, wanted, actual, None, guard, state).await
}

async fn run_lifecycle<G: Clone + Send + 'static>(
  key: &str,
  options: &VmLifecycleOptions,
  restart: bool,
  expected_id: Option<&str>,
  owner: Option<OperationOwner>,
  guard: G,
  state: &SystemState,
) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
  if crate::models::OperationDb::current("Vm", key, &state.inner.pool)
    .await?
    .is_some_and(|operation| {
      operation.outcome.is_none() && Some(operation.owner()) != owner
    })
  {
    return Err(conflict(
      "A VM lifecycle operation is awaiting completion or recovery",
    ));
  }
  if let Some(expected) = expected_id {
    let runtime = inspect_runtime(&vm, state)
      .await?
      .ok_or_else(|| conflict("Requested VM process no longer exists"))?;
    if runtime.id.as_deref() != Some(expected) {
      return Err(conflict("Requested VM process has been replaced"));
    }
  }
  if owner.is_none()
    && matches!(
      vm.status.actual,
      ObjPsStatusKind::Starting
        | ObjPsStatusKind::Updating
        | ObjPsStatusKind::Destroying
    )
  {
    return Err(conflict("A VM lifecycle transition is pending"));
  }
  set_status(
    key,
    ObjPsStatusKind::Stop,
    ObjPsStatusKind::Stopping,
    owner.clone(),
    guard.clone(),
    state,
  )
  .await?;
  if let Err(err) = complete_lifecycle(
    &vm,
    options,
    restart,
    owner.clone(),
    guard.clone(),
    state,
  )
  .await
  {
    // Every failure after recording the stop request must restore observed
    // state, including database failures after the guest has already stopped.
    // Durable restart failures are observed and settled by their owning task.
    if owner.is_none()
      && let Err(restore_error) =
        restore_observed_status(&vm, guard, state).await
    {
      log::error!(
        "Unable to restore VM {key} status after {err}: {restore_error}"
      );
    }
    return Err(err);
  }
  if restart {
    state
      .emit_normal_native_action_sync(&vm, NativeEventAction::Restart)
      .await;
  } else {
    state
      .emit_normal_native_action_sync(&vm, NativeEventAction::Stop)
      .await;
  }
  Ok(())
}

async fn complete_lifecycle<G: Clone + Send + 'static>(
  vm: &Vm,
  options: &VmLifecycleOptions,
  restart: bool,
  owner: Option<OperationOwner>,
  guard: G,
  state: &SystemState,
) -> IoResult<()> {
  let key = &vm.spec.vm_key;
  let id = stop_runtime(vm, options, state).await?;
  set_status(
    key,
    ObjPsStatusKind::Stop,
    ObjPsStatusKind::Stop,
    owner.clone(),
    guard.clone(),
    state,
  )
  .await?;
  if restart {
    set_status(
      key,
      ObjPsStatusKind::Start,
      ObjPsStatusKind::Starting,
      owner.clone(),
      guard.clone(),
      state,
    )
    .await?;
    timeout_at(Instant::now() + Duration::from_secs(30), async {
      if let Some(id) = id {
        // Reuse the verified immutable runtime identity, never a mutable name.
        state
          .inner
          .docker_api
          .start_container(&id, None::<StartContainerOptions>)
          .await
          .map_err(|err| err.map_err_context(|| "VM restart"))?;
        let runtime = state
          .inner
          .docker_api
          .inspect_container(&id, None)
          .await
          .map_err(|err| err.map_err_context(|| "VM restart confirmation"))?;
        if runtime.id.as_deref() != Some(&id) || !is_running(&runtime)? {
          return Err(IoError::interrupted(
            "VM restart",
            "Runtime did not remain running after start",
          ));
        }
        ObjPsStatusDb::update_actual_status_owned(
          key,
          &ObjPsStatusKind::Start,
          owner.clone(),
          guard.clone(),
          &state.inner.pool,
        )
        .await?;
      } else {
        super::start_unlocked_owned(key, owner.clone(), guard.clone(), state)
          .await?;
      }
      Ok::<_, IoError>(())
    })
    .await
    .unwrap_or_else(|_| {
      Err(IoError::with_context(
        "VM restart",
        std::io::Error::new(
          std::io::ErrorKind::TimedOut,
          "Runtime start was not confirmed; inspect VM status before retrying",
        ),
      ))
    })?;
  }
  Ok(())
}

pub(crate) async fn run_restart_owned(
  key: &str,
  options: &VmLifecycleOptions,
  expected_id: Option<&str>,
  operation: &OperationDb,
  lease: &VmOperationLease,
  state: &SystemState,
) -> (IoResult<()>, bool) {
  let owner = Some(operation.owner());
  let result = run_lifecycle(
    key,
    options,
    true,
    expected_id,
    owner.clone(),
    lease.clone(),
    state,
  )
  .await;
  super::record_task_outcome_owned(key, result, owner, lease.clone(), state)
    .await
}

async fn begin_lifecycle(
  key: &str,
  options: &VmLifecycleOptions,
  restart: bool,
  expected_id: Option<&str>,
  state: &SystemState,
) -> IoResult<tokio::sync::oneshot::Receiver<IoResult<()>>> {
  options
    .validate()
    .map_err(|err| IoError::invalid_input("VM shutdown", &err))?;
  if restart {
    let (completion, receiver) = tokio::sync::oneshot::channel();
    crate::tasks::vm::accept_restart(
      key,
      VmRestartTask {
        options: options.clone(),
        expected_id: expected_id.map(str::to_owned),
        completion,
      },
      state,
    )
    .await
    .map_err(|error| {
      // Acceptance uses the HTTP task facade; retain the lifecycle endpoint's
      // existing conflict, missing-resource and timeout responses on return.
      let kind = match error.status {
        ntex::http::StatusCode::CONFLICT => std::io::ErrorKind::WouldBlock,
        ntex::http::StatusCode::NOT_FOUND => std::io::ErrorKind::NotFound,
        ntex::http::StatusCode::BAD_REQUEST => std::io::ErrorKind::InvalidInput,
        ntex::http::StatusCode::GATEWAY_TIMEOUT => std::io::ErrorKind::TimedOut,
        _ => std::io::ErrorKind::Other,
      };
      IoError::with_context(
        "VM restart acceptance",
        std::io::Error::new(kind, error),
      )
    })?;
    return Ok(receiver);
  }
  let guard = Arc::new(lifecycle_lock(key).try_lock_owned().await?);
  // Validate existence before an asynchronous process endpoint accepts work.
  let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let key = key.to_owned();
  let options = options.clone();
  let state = state.clone();
  let expected_id = expected_id.map(str::to_owned);
  // Finish bounded shutdown/status bookkeeping even if the HTTP caller leaves.
  // No automatic retry is made after a timeout or an uncertain QMP response.
  let (tx, rx) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let guard = guard;
    let result = run_lifecycle(
      &key,
      &options,
      restart,
      expected_id.as_deref(),
      None,
      guard.clone(),
      &state,
    )
    .await;
    if let Err(error) = &result {
      state.emit_error_native_action(
        &vm,
        if restart {
          NativeEventAction::Restart
        } else {
          NativeEventAction::Stop
        },
        Some(error.to_string()),
      );
    }
    // The completion receiver must not wake while mutation admission is held.
    drop(guard);
    let _ = tx.send(result);
  });
  Ok(rx)
}

async fn lifecycle(
  key: &str,
  options: &VmLifecycleOptions,
  restart: bool,
  expected_id: Option<&str>,
  state: &SystemState,
) -> IoResult<()> {
  let rx = begin_lifecycle(key, options, restart, expected_id, state).await?;
  rx.await.map_err(|_| {
    IoError::interrupted("VM lifecycle", "Lifecycle task was interrupted")
  })?
}

/// Preserve the process API's asynchronous 202 contract. The owned task and
/// lifecycle guard survive dropping the completion receiver.
pub(crate) async fn enqueue_lifecycle(
  key: &str,
  expected_id: Option<&str>,
  restart: bool,
  state: &SystemState,
) -> IoResult<()> {
  drop(
    begin_lifecycle(key, &Default::default(), restart, expected_id, state)
      .await?,
  );
  Ok(())
}

pub(crate) async fn stop_with_options(
  key: &str,
  options: &VmLifecycleOptions,
  state: &SystemState,
) -> IoResult<()> {
  lifecycle(key, options, false, None, state).await
}

pub(crate) async fn restart_with_options(
  key: &str,
  options: &VmLifecycleOptions,
  state: &SystemState,
) -> IoResult<()> {
  lifecycle(key, options, true, None, state).await
}

#[cfg(test)]
mod tests {
  use super::*;

  fn fixture() -> (Vm, ContainerInspectResponse) {
    let mut vm = Vm::default();
    vm.spec.vm_key = "global.test".into();
    vm.namespace_name = "global".into();
    let generation = uuid::Uuid::nil();
    let runtime = serde_json::from_value(serde_json::json!({
      "Id":"immutable-id", "Name":"/global.test.v", "State":{"Running":true},
      "Config":{"Labels":{"io.nanocl.v":"global.test","io.nanocl.n":"global","io.nanocl.vm-spec":generation.to_string(),"io.nanocl.vm-qmp":"1"}},
      "Mounts":[{"Type":"bind","Source":format!("/run/vms/global.test/{generation}"),"Destination":"/run/nanocl-vm","RW":true}]
    })).unwrap();
    (vm, runtime)
  }

  #[test]
  fn vm_qmp_socket_requires_owned_generation_and_private_bind() {
    let (mut vm, runtime) = fixture();
    vm.spec.key = uuid::Uuid::new_v4();
    assert_eq!(
      socket_directory(&vm, &runtime, "/").unwrap(),
      PathBuf::from(
        "/run/vms/global.test/00000000-0000-0000-0000-000000000000"
      )
    );
    for (field, value) in [
      ("io.nanocl.v", "global.other"),
      ("io.nanocl.n", "other"),
      ("io.nanocl.vm-spec", "../escape"),
      ("io.nanocl.vm-qmp", "0"),
    ] {
      let mut changed = runtime.clone();
      changed
        .config
        .as_mut()
        .unwrap()
        .labels
        .as_mut()
        .unwrap()
        .insert(field.into(), value.into());
      assert!(socket_directory(&vm, &changed, "/").is_err(), "{field}");
    }
    let mut changed = runtime.clone();
    changed.mounts.as_mut().unwrap()[0].source =
      Some("/run/vms/global.other/00000000-0000-0000-0000-000000000000".into());
    assert!(socket_directory(&vm, &changed, "/").is_err());
    changed = runtime.clone();
    changed.mounts.as_mut().unwrap()[0].rw = Some(false);
    assert!(socket_directory(&vm, &changed, "/").is_err());
    for typ in [None, Some("volume"), Some("future-type")] {
      let mut changed = runtime.clone();
      changed.mounts.as_mut().unwrap()[0].typ = typ.map(str::to_owned);
      assert!(socket_directory(&vm, &changed, "/").is_err());
    }
    assert!(socket_directory(&vm, &runtime, "relative").is_err());
  }

  #[ntex::test]
  async fn vm_qmp_lifecycle_lock_is_shared_per_vm_and_released() {
    let key = "global.lock-test-a";
    // No caller retains the Arc: the owned guard must keep the weak registry
    // entry alive so a fresh lookup cannot bypass the current owner.
    let guard = lifecycle_lock(key).try_lock_owned().await.unwrap();
    assert!(lifecycle_lock(key).try_lock_owned().await.is_err());
    assert!(
      lifecycle_lock("global.lock-test-b")
        .try_lock_owned()
        .await
        .is_ok()
    );
    drop(guard);
    assert!(lifecycle_lock(key).try_lock_owned().await.is_ok());
  }

  #[ntex::test]
  async fn vm_lifecycle_completed_start_waits_for_exit_observation() {
    use std::{
      cell::RefCell,
      future::Future,
      task::{Context, Poll},
    };

    for runtime_running in [true, false] {
      let key = format!("global.awaited-start-{runtime_running}");
      let observed = RefCell::new(ObjPsStatusKind::Start);
      let start = lifecycle_lock(&key).try_lock_owned().await.unwrap();
      // The native Start barrier has completed, then the Docker observer wins
      // serialization before the client's immediate forced-stop request.
      drop(start);
      let observer = lifecycle_lock(&key).try_observe_owned().unwrap();
      let mut stop = Box::pin(async {
        let guard = lifecycle_lock(&key).try_lock_owned().await?;
        Ok::<_, IoError>((guard, observed.borrow().clone()))
      });
      let mut context = Context::from_waker(futures::task::noop_waker_ref());
      assert!(stop.as_mut().poll(&mut context).is_pending());
      // A real competing mutation must still reject, including while the
      // admitted stop waits for observation's protected status write.
      assert!(lifecycle_lock(&key).try_lock_owned().await.is_err());
      *observed.borrow_mut() = if runtime_running {
        ObjPsStatusKind::Start
      } else {
        ObjPsStatusKind::Fail
      };
      assert!(stop.as_mut().poll(&mut context).is_pending());
      drop(observer);
      let Poll::Ready(Ok((stop_guard, observed_after_acquisition))) =
        stop.as_mut().poll(&mut context)
      else {
        panic!("stop must acquire only after the observation finishes");
      };
      // Both running and already-exited observations permit stop admission;
      // the stop operation then inspects runtime identity/state under the guard.
      assert_eq!(
        observed_after_acquisition,
        if runtime_running {
          ObjPsStatusKind::Start
        } else {
          ObjPsStatusKind::Fail
        }
      );
      assert!(lifecycle_lock(&key).try_observe_owned().is_none());
      drop(stop_guard);
      assert!(lifecycle_lock(&key).try_lock_owned().await.is_ok());
    }
  }

  #[ntex::test]
  async fn vm_lifecycle_cancelled_observation_wait_releases_only_admission() {
    use std::{future::Future, task::Context};

    let key = "global.cancelled-observation-wait";
    let observer = lifecycle_lock(key).try_observe_owned().unwrap();
    let mut stop = Box::pin(lifecycle_lock(key).try_lock_owned());
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    assert!(stop.as_mut().poll(&mut context).is_pending());
    drop(stop);
    assert!(lifecycle_lock(key).try_observe_owned().is_none());
    assert!(
      lifecycle_lock(key)
        .mutation
        .clone()
        .try_lock_owned()
        .is_ok()
    );
    drop(observer);
    assert!(lifecycle_lock(key).try_lock_owned().await.is_ok());
  }

  #[ntex::test]
  async fn vm_lifecycle_expired_observation_wait_does_not_release_observer() {
    let key = "global.expired-observation-wait";
    let observer = lifecycle_lock(key).try_observe_owned().unwrap();
    let result = lifecycle_lock(key)
      .try_lock_owned_before(Instant::now())
      .await;
    let Err(error) = result else {
      panic!("admission must expire while observation owns the write");
    };
    assert_eq!(error.inner.kind(), std::io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("observation"));
    assert!(lifecycle_lock(key).try_observe_owned().is_none());
    assert!(
      lifecycle_lock(key)
        .mutation
        .clone()
        .try_lock_owned()
        .is_ok()
    );
    drop(observer);
    assert!(lifecycle_lock(key).try_lock_owned().await.is_ok());
  }

  #[test]
  fn vm_qmp_unknown_container_state_is_not_stopped() {
    let (_, mut runtime) = fixture();
    assert_eq!(is_running(&runtime).unwrap(), true);
    runtime.state.as_mut().unwrap().running = Some(false);
    assert_eq!(is_running(&runtime).unwrap(), false);
    runtime.state.as_mut().unwrap().running = None;
    assert!(is_running(&runtime).is_err());
    runtime.state = None;
    assert!(is_running(&runtime).is_err());
  }

  #[test]
  fn vm_qmp_process_identity_rejects_other_nodes_and_replaced_ids() {
    let (vm, runtime) = fixture();
    let process = Process {
      key: "immutable-id".into(),
      name: "global.test.v".into(),
      kind: ProcessKind::Vm,
      kind_key: "global.test".into(),
      node_name: "local".into(),
      created_at: Default::default(),
      updated_at: Default::default(),
      ip_address: None,
      data: runtime.clone(),
    };
    assert!(
      validate_process_owner(&vm, &runtime, &[process.clone()], "local")
        .is_ok()
    );
    assert!(
      validate_process_owner(&vm, &runtime, &[process.clone()], "remote")
        .is_err()
    );
    let mut changed = process.clone();
    changed.key = "replacement-id".into();
    assert!(
      validate_process_owner(&vm, &runtime, &[changed], "local").is_err()
    );
    let mut changed = process;
    changed.kind_key = "global.other".into();
    assert!(
      validate_process_owner(&vm, &runtime, &[changed], "local").is_err()
    );
  }
}
