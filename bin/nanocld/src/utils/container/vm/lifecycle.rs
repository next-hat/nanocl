use std::{
  os::unix::fs::{FileTypeExt, PermissionsExt},
  path::PathBuf,
  sync::{Arc, OnceLock},
  time::Duration,
};

use bollard_next::{
  container::{
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
  },
  models::ContainerInspectResponse,
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
    ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb, QmpClient, SystemState, VmDb,
    VmLifecycleLocks,
  },
  repositories::generic::*,
};

pub(crate) fn lifecycle_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
  static LOCKS: OnceLock<VmLifecycleLocks> = OnceLock::new();
  let mut locks = LOCKS
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|err| err.into_inner());
  locks.retain(|_, lock| lock.strong_count() > 0);
  if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
    return lock;
  }
  let lock = Arc::new(tokio::sync::Mutex::new(()));
  locks.insert(key.to_owned(), Arc::downgrade(&lock));
  lock
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

async fn inspect_runtime(
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
  Ok(Some(runtime))
}

fn socket_directory(
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
      "This runtime lacks managed QMP; explicitly force-stop it before upgrading to a QMP-capable image",
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
  let directory = PathBuf::from(state_dir)
    .join("run/vms")
    .join(key.to_string())
    .join(generation.to_string());
  if !directory.is_absolute()
    || directory.join("qmp.sock").as_os_str().len() >= 108
  {
    return Err(conflict("Invalid or excessively long VM socket directory"));
  }
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
    || mount.typ != Some(bollard_next::models::MountPointTypeEnum::BIND)
  {
    return Err(conflict(
      "VM runtime socket mount does not match its owner and generation",
    ));
  }
  Ok(directory)
}

async fn connect(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  state: &SystemState,
  deadline: Instant,
) -> IoResult<QmpClient> {
  let directory = socket_directory(vm, runtime, &state.inner.config.state_dir)?;
  if tokio::fs::canonicalize(&directory).await? != directory {
    return Err(conflict(
      "VM socket directory must not redirect through symlinks",
    ));
  }
  let metadata = tokio::fs::symlink_metadata(&directory).await?;
  if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
    return Err(conflict("VM socket directory must be private"));
  }
  let socket = directory.join("qmp.sock");
  if !tokio::fs::symlink_metadata(&socket)
    .await?
    .file_type()
    .is_socket()
  {
    return Err(conflict("VM QMP path must be a Unix socket"));
  }
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
      .stop_container(&id, Some(StopContainerOptions { t: 0 }))
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
      condition: "not-running",
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

async fn set_status(
  key: &str,
  wanted: ObjPsStatusKind,
  actual: ObjPsStatusKind,
  state: &SystemState,
) -> IoResult<()> {
  let old = ObjPsStatusDb::read_by_pk(key, &state.inner.pool).await?;
  ObjPsStatusDb::update_pk(
    key,
    ObjPsStatusUpdate {
      wanted: Some(wanted.to_string()),
      prev_wanted: Some(old.wanted),
      actual: Some(actual.to_string()),
      prev_actual: Some(old.actual),
      ..Default::default()
    },
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

async fn restore_observed_status(vm: &Vm, state: &SystemState) -> IoResult<()> {
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
  set_status(&vm.spec.vm_key, wanted, actual, state).await
}

async fn run_lifecycle(
  key: &str,
  options: &VmLifecycleOptions,
  restart: bool,
  expected_id: Option<&str>,
  state: &SystemState,
) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
  if let Some(expected) = expected_id {
    let runtime = inspect_runtime(&vm, state)
      .await?
      .ok_or_else(|| conflict("Requested VM process no longer exists"))?;
    if runtime.id.as_deref() != Some(expected) {
      return Err(conflict("Requested VM process has been replaced"));
    }
  }
  if matches!(
    vm.status.actual,
    ObjPsStatusKind::Starting
      | ObjPsStatusKind::Updating
      | ObjPsStatusKind::Destroying
  ) {
    return Err(conflict("A VM lifecycle transition is pending"));
  }
  set_status(key, ObjPsStatusKind::Stop, ObjPsStatusKind::Stopping, state)
    .await?;
  let stopped = stop_runtime(&vm, options, state).await;
  let id = match stopped {
    Ok(id) => id,
    Err(err) => {
      // A timeout must not mark a live guest stopped or leave a stop request
      // queued for reconciliation to retry later without the operator.
      restore_observed_status(&vm, state).await?;
      state.emit_error_native_action(
        &vm,
        if restart {
          NativeEventAction::Restart
        } else {
          NativeEventAction::Stop
        },
        Some(err.to_string()),
      );
      return Err(err);
    }
  };
  set_status(key, ObjPsStatusKind::Stop, ObjPsStatusKind::Stop, state).await?;
  if restart {
    set_status(
      key,
      ObjPsStatusKind::Start,
      ObjPsStatusKind::Starting,
      state,
    )
    .await?;
    let result = timeout_at(Instant::now() + Duration::from_secs(30), async {
      if let Some(id) = id {
        // Reuse the verified immutable runtime identity, never a mutable name.
        state
          .inner
          .docker_api
          .start_container(&id, None::<StartContainerOptions<String>>)
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
        ObjPsStatusDb::update_actual_status(
          key,
          &ObjPsStatusKind::Start,
          &state.inner.pool,
        )
        .await?;
      } else {
        super::start_unlocked(key, state).await?;
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
    });
    if let Err(err) = result {
      restore_observed_status(&vm, state).await?;
      state.emit_error_native_action(
        &vm,
        NativeEventAction::Restart,
        Some(err.to_string()),
      );
      return Err(err);
    }
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

async fn lifecycle(
  key: &str,
  options: &VmLifecycleOptions,
  restart: bool,
  expected_id: Option<&str>,
  state: &SystemState,
) -> IoResult<()> {
  options
    .validate()
    .map_err(|err| IoError::invalid_input("VM shutdown", &err))?;
  let guard = lifecycle_lock(key)
    .try_lock_owned()
    .map_err(|_| conflict("A VM lifecycle operation is active"))?;
  let key = key.to_owned();
  let options = options.clone();
  let state = state.clone();
  let expected_id = expected_id.map(str::to_owned);
  // Finish bounded shutdown/status bookkeeping even if the HTTP caller leaves.
  // No automatic retry is made after a timeout or an uncertain QMP response.
  let (tx, rx) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let _guard = guard;
    let result =
      run_lifecycle(&key, &options, restart, expected_id.as_deref(), &state)
        .await;
    let _ = tx.send(result);
  });
  rx.await.map_err(|_| {
    IoError::interrupted("VM lifecycle", "Lifecycle task was interrupted")
  })?
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

pub(crate) async fn control_process(
  key: &str,
  id: &str,
  restart: bool,
  state: &SystemState,
) -> IoResult<()> {
  lifecycle(key, &Default::default(), restart, Some(id), state).await
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
      "Mounts":[{"Type":"bind","Source":format!("/state/run/vms/global.test/{generation}"),"Destination":"/run/nanocl-vm","RW":true}]
    })).unwrap();
    (vm, runtime)
  }

  #[test]
  fn vm_qmp_socket_requires_owned_generation_and_private_bind() {
    let (mut vm, runtime) = fixture();
    vm.spec.key = uuid::Uuid::new_v4();
    assert_eq!(
      socket_directory(&vm, &runtime, "/state").unwrap(),
      PathBuf::from(
        "/state/run/vms/global.test/00000000-0000-0000-0000-000000000000"
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
      assert!(
        socket_directory(&vm, &changed, "/state").is_err(),
        "{field}"
      );
    }
    let mut changed = runtime.clone();
    changed.mounts.as_mut().unwrap()[0].source = Some(
      "/state/run/vms/global.other/00000000-0000-0000-0000-000000000000".into(),
    );
    assert!(socket_directory(&vm, &changed, "/state").is_err());
    changed = runtime.clone();
    changed.mounts.as_mut().unwrap()[0].rw = Some(false);
    assert!(socket_directory(&vm, &changed, "/state").is_err());
    assert!(socket_directory(&vm, &runtime, "relative").is_err());
  }

  #[test]
  fn vm_qmp_lifecycle_lock_is_shared_per_vm_and_released() {
    let a = lifecycle_lock("global.lock-test-a");
    let b = lifecycle_lock("global.lock-test-a");
    let other = lifecycle_lock("global.lock-test-b");
    let guard = a.try_lock().unwrap();
    assert!(b.try_lock().is_err());
    assert!(other.try_lock().is_ok());
    drop(guard);
    assert!(b.try_lock().is_ok());
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
