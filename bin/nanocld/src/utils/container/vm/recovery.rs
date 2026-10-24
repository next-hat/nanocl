use std::{
  ffi::CString,
  fs::{File, OpenOptions},
  os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
  },
  sync::Arc,
  time::Duration,
};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
  vm::Vm,
};
use serde_json::{Value, json};

use crate::{
  models::{
    ProcessDb, SpecDb, SystemState, VmDb, VmOperationDb, VmOperationGuard,
    VmOperationOwner,
  },
  repositories::generic::*,
};

fn conflict(message: &str) -> IoError {
  IoError::invalid_data("VM operation recovery", message)
}

fn boot_id() -> IoResult<String> {
  let value = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
  let value = value.trim();
  uuid::Uuid::parse_str(value)
    .map_err(|_| conflict("Linux boot identity is invalid"))?;
  Ok(value.to_owned())
}

// Canonical state-root aliases are intentional. Every component below that
// trusted root is opened without following symlinks and stays pinned by a fd.
fn operation_directory(state_dir: &str, create: bool) -> IoResult<File> {
  let root = std::fs::canonicalize(state_dir)?;
  let mut directory = OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(root)?;
  for name in ["run", "vm-operations"] {
    let name = CString::new(name).unwrap();
    if create {
      let result =
        unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
      if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
          return Err(error.into());
        }
      }
      directory.sync_all()?;
    }
    let fd = unsafe {
      libc::openat(
        directory.as_raw_fd(),
        name.as_ptr(),
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
      )
    };
    if fd < 0 {
      return Err(std::io::Error::last_os_error().into());
    }
    directory = unsafe { File::from_raw_fd(fd) };
  }
  Ok(directory)
}

fn lock_file(
  directory: &File,
  key: uuid::Uuid,
  create: bool,
) -> IoResult<File> {
  let name = CString::new(format!("{key}.lock")).unwrap();
  let flags = libc::O_RDWR
    | libc::O_NOFOLLOW
    | libc::O_CLOEXEC
    | if create {
      libc::O_CREAT | libc::O_EXCL
    } else {
      0
    };
  let fd =
    unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags, 0o600) };
  if fd < 0 {
    return Err(std::io::Error::last_os_error().into());
  }
  let file = unsafe { File::from_raw_fd(fd) };
  if !file.metadata()?.is_file() {
    return Err(conflict("Operation lock is not a regular file"));
  }
  Ok(file)
}

fn try_lock(file: &File) -> IoResult<bool> {
  if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }
    == 0
  {
    return Ok(true);
  }
  let error = std::io::Error::last_os_error();
  if error.kind() == std::io::ErrorKind::WouldBlock {
    return Ok(false);
  }
  Err(error.into())
}

pub(crate) fn create_owner(
  key: uuid::Uuid,
  state: &SystemState,
) -> IoResult<(VmOperationOwner, VmOperationGuard)> {
  create_owner_at(
    key,
    &state.inner.config.hostname,
    &state.inner.config.state_dir,
  )
}

fn create_owner_at(
  key: uuid::Uuid,
  node_name: &str,
  state_dir: &str,
) -> IoResult<(VmOperationOwner, VmOperationGuard)> {
  let owner_boot_id = boot_id()?;
  let directory = operation_directory(state_dir, true)?;
  let file = lock_file(&directory, key, true)?;
  if !try_lock(&file)? {
    return Err(conflict("New operation lock is already owned"));
  }
  let metadata = file.metadata()?;
  file.sync_all()?;
  directory.sync_all()?;
  let owner = VmOperationOwner {
    key,
    node_name: node_name.to_owned(),
    owner_boot_id,
    lock_device: metadata.dev() as i64,
    lock_inode: metadata.ino() as i64,
  };
  Ok((
    owner,
    VmOperationGuard {
      _file: Arc::new(file),
    },
  ))
}

fn abandoned_owner(
  operation: &VmOperationDb,
  node_name: &str,
  state_dir: &str,
) -> IoResult<Option<VmOperationGuard>> {
  // Missing files, foreign nodes, and changed boot identities do not establish
  // that an owner on another filesystem/process namespace has stopped.
  if operation.node_name != node_name || operation.owner_boot_id != boot_id()? {
    return Ok(None);
  }
  let directory = match operation_directory(state_dir, false) {
    Ok(directory) => directory,
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      return Ok(None);
    }
    Err(error) => return Err(error),
  };
  let file = match lock_file(&directory, operation.key, false) {
    Ok(file) => file,
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      return Ok(None);
    }
    Err(error) => return Err(error),
  };
  let metadata = file.metadata()?;
  if metadata.dev() as i64 != operation.lock_device
    || metadata.ino() as i64 != operation.lock_inode
    || !try_lock(&file)?
  {
    return Ok(None);
  }
  Ok(Some(VmOperationGuard {
    _file: Arc::new(file),
  }))
}

fn same_operation(left: &VmOperationDb, right: &VmOperationDb) -> bool {
  left.vm_key == right.vm_key
    && left.key == right.key
    && left.spec_key == right.spec_key
    && left.phase == right.phase
    && left.node_name == right.node_name
    && left.owner_boot_id == right.owner_boot_id
    && left.lock_device == right.lock_device
    && left.lock_inode == right.lock_inode
    && left.outcome.is_none()
    && right.outcome.is_none()
}

async fn observe(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<(ObjPsStatusKind, Value)> {
  let processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  if processes.iter().any(|process| {
    process.kind != ProcessKind::Vm
      || process.node_name != state.inner.config.hostname
      || process.key.is_empty()
  }) {
    return Err(conflict(
      "Runtime process ownership is not local and unambiguous",
    ));
  }
  ensure_auxiliary_stopped(vm, state).await?;
  let name = format!("{}.v", vm.spec.vm_key);
  let processes = processes
    .iter()
    .filter(|process| process.name == name)
    .collect::<Vec<_>>();
  let runtime = super::lifecycle::inspect_runtime(vm, state).await?;
  let Some(runtime) = runtime else {
    // A mutable-name miss must not hide an immutable runtime that was renamed.
    for process in processes {
      match state
        .inner
        .docker_api
        .inspect_container(&process.key, None)
        .await
      {
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => {}
        Ok(_) => {
          return Err(conflict(
            "An owned runtime still exists under a different name",
          ));
        }
        Err(error) => {
          return Err(conflict(&format!(
            "Runtime inspection unavailable: {error}"
          )));
        }
      }
    }
    return Ok((
      ObjPsStatusKind::Stop,
      json!({"Runtime": "absent", "ContainerId": null, "Specification": null, "StartedAt": null}),
    ));
  };
  let generation = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref())
    .and_then(|labels| labels.get("io.nanocl.vm-spec"))
    .and_then(|value| uuid::Uuid::parse_str(value).ok())
    .ok_or_else(|| {
      conflict("Runtime specification generation is missing or invalid")
    })?;
  let boot = SpecDb::read_by_pk(&generation, &state.inner.pool).await?;
  if boot.kind_name != "Vm" || boot.kind_key != vm.spec.vm_key {
    return Err(conflict("Runtime specification belongs to another object"));
  }
  let running = runtime
    .state
    .as_ref()
    .and_then(|state| state.running)
    .ok_or_else(|| conflict("Runtime running state is unknown"))?;
  let started_at = runtime
    .state
    .as_ref()
    .and_then(|state| state.started_at.as_deref())
    .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
    .ok_or_else(|| conflict("Runtime current-run identity is unknown"))?;
  let id = super::owned_runtime_id(vm, &runtime)?;
  let confirmed = state
    .inner
    .docker_api
    .inspect_container(&id, None)
    .await
    .map_err(|error| {
      conflict(&format!("Runtime confirmation unavailable: {error}"))
    })?;
  if super::owned_runtime_id(vm, &confirmed)? != id
    || confirmed.state.as_ref().and_then(|state| state.running) != Some(running)
    || confirmed
      .state
      .as_ref()
      .and_then(|state| state.started_at.as_deref())
      != Some(started_at)
  {
    return Err(conflict(
      "Runtime identity or current run changed during recovery",
    ));
  }
  Ok((
    if running {
      ObjPsStatusKind::Start
    } else {
      ObjPsStatusKind::Stop
    },
    json!({
      "Runtime": if running {"running"} else {"stopped"},
      "ContainerId": runtime.id,
      "Specification": generation,
      "AcceptedSpecification": vm.spec.key,
      "AcceptedGenerationObserved": generation == vm.spec.key,
      "StartedAt": runtime.state.as_ref().and_then(|state| state.started_at.clone()),
    }),
  ))
}

// A daemon can disappear while an init container is still provisioning a disk.
// Include containers whose Process row was never written, and temporary or
// renamed VM containers. Their continued execution makes a retry unsafe.
async fn ensure_auxiliary_stopped(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<()> {
  let containers = state
    .inner
    .docker_api
    .list_containers(Some(bollard_next::container::ListContainersOptions::<
      String,
    > {
      all: true,
      filters: std::collections::HashMap::from([(
        "label".to_owned(),
        vec![format!("io.nanocl.v={}", vm.spec.vm_key)],
      )]),
      ..Default::default()
    }))
    .await
    .map_err(|error| {
      conflict(&format!("VM container inventory unavailable: {error}"))
    })?;
  for container in containers {
    let id = container.id.filter(|id| !id.is_empty()).ok_or_else(|| {
      conflict("VM container inventory has no immutable identity")
    })?;
    let runtime =
      match state.inner.docker_api.inspect_container(&id, None).await {
        Ok(runtime) => runtime,
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => continue,
        Err(error) => {
          return Err(conflict(&format!(
            "VM auxiliary inspection unavailable: {error}"
          )));
        }
      };
    let labels = runtime
      .config
      .as_ref()
      .and_then(|config| config.labels.as_ref());
    if runtime.id.as_deref() != Some(&id)
      || labels.and_then(|labels| labels.get("io.nanocl.v"))
        != Some(&vm.spec.vm_key)
      || labels.and_then(|labels| labels.get("io.nanocl.n"))
        != Some(&vm.namespace_name)
    {
      return Err(conflict("VM auxiliary container ownership is ambiguous"));
    }
    if runtime.name.as_deref() == Some(&format!("/{}.v", vm.spec.vm_key))
      && labels
        .and_then(|labels| labels.get("io.nanocl.init-v"))
        .map(String::as_str)
        != Some("true")
    {
      continue;
    }
    if runtime.state.as_ref().and_then(|state| state.running) != Some(false) {
      return Err(conflict(&format!(
        "Owned VM auxiliary container {id} ({:?}) is active or unknown; wait for it to exit or explicitly stop it before retrying",
        runtime.name
      )));
    }
  }
  Ok(())
}

/// Settle exactly one abandoned acceptance. This never starts, stops, replaces,
/// or deletes a runtime. Do not timeout the final database write: its blocking
/// worker must finish before the file and lifecycle guards can be released.
pub(crate) async fn recover_one(
  candidate: &VmOperationDb,
  state: &SystemState,
) -> IoResult<bool> {
  let lifecycle = super::lifecycle_lock(&candidate.vm_key);
  let _lifecycle = match tokio::time::timeout(
    Duration::from_millis(50),
    lifecycle.try_lock_owned(),
  )
  .await
  {
    Ok(Ok(guard)) => guard,
    _ => return Ok(false),
  };
  let Some(_owner) = abandoned_owner(
    candidate,
    &state.inner.config.hostname,
    &state.inner.config.state_dir,
  )?
  else {
    return Ok(false);
  };
  let Some(current) =
    VmOperationDb::get(&candidate.vm_key, &state.inner.pool).await?
  else {
    return Ok(false);
  };
  if !same_operation(candidate, &current) {
    return Ok(false);
  }
  let vm =
    VmDb::transform_read_by_pk(&candidate.vm_key, &state.inner.pool).await?;
  if vm.spec.key != candidate.spec_key {
    return Ok(false);
  }
  let action = match candidate.phase.as_str() {
    "starting" => NativeEventAction::Starting,
    "updating" => NativeEventAction::Updating,
    "destroying" => NativeEventAction::Destroying,
    _ => return Err(conflict("Unsupported pending phase")),
  };
  let (actual, mut observation) =
    tokio::time::timeout(Duration::from_secs(5), observe(&vm, state))
      .await
      .map_err(|_| {
        conflict("Runtime observation timed out; recovery remains pending")
      })??;
  observation["Failure"] = json!(
    "The accepted operation lost its owner; no automatic replay was attempted"
  );
  observation["Operation"] = json!(candidate.key);
  observation["Phase"] = json!(candidate.phase);
  let finished = current
    .finish(actual, "interrupted", observation, &state.inner.pool)
    .await?;
  if finished {
    state.emit_error_native_action(&vm, action,
      Some(format!("VM operation {} ({}) was interrupted; observed runtime reconciled without replay. Inspect the VM and explicitly retry the request", candidate.key, candidate.phase)));
  }
  Ok(finished)
}

/// Startup and background passes share the same idempotent recovery path.
/// A cursor prevents unclassifiable early rows from starving later operations.
pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  ntex::rt::spawn(async move {
    let mut after: Option<String> = None;
    loop {
      match recover_pending(&state, after.as_deref()).await {
        Ok(cursor) => after = cursor,
        Err(error) => log::warn!("VM recovery scan deferred: {error}"),
      }
      ntex::time::sleep(Duration::from_secs(10)).await;
    }
  });
}

pub(crate) async fn recover_pending(
  state: &SystemState,
  after: Option<&str>,
) -> IoResult<Option<String>> {
  let candidates = VmOperationDb::pending_for_node(
    &state.inner.config.hostname,
    after,
    32,
    &state.inner.pool,
  )
  .await?;
  let cursor = if candidates.len() == 32 {
    candidates.last().map(|operation| operation.vm_key.clone())
  } else {
    None
  };
  for candidate in candidates {
    if let Err(error) = recover_one(&candidate, state).await {
      log::warn!("VM {} recovery deferred: {error}", candidate.vm_key);
    }
  }
  Ok(cursor)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::{
    DistributedMutexDb, DistributedMutexPartial, Pool, SystemStateInner,
  };
  use futures::FutureExt;
  use nanocl_stubs::vm_spec::{VmHostConfig, VmSpecPartial};

  struct LockDirectory(std::path::PathBuf);

  impl LockDirectory {
    fn new() -> Self {
      let path = std::env::temp_dir()
        .join(format!("nanocl-vm-owner-{}", uuid::Uuid::new_v4()));
      std::fs::create_dir(&path).unwrap();
      Self(path)
    }

    fn path(&self) -> &str {
      self.0.to_str().unwrap()
    }
  }

  impl Drop for LockDirectory {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  #[test]
  fn vm_operation_lock_preserves_live_owner_and_all_clones() {
    let directory = LockDirectory::new();
    let key = uuid::Uuid::new_v4();
    let (owner, guard) =
      create_owner_at(key, "local", directory.path()).unwrap();
    let operation =
      owner.operation("global.owner", uuid::Uuid::new_v4(), "starting");
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_none()
    );
    let detached = guard.clone();
    drop(guard);
    // No mutex row or TaskManager entry is consulted. The last detached owner
    // remains authoritative regardless of TTL or registration stage.
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_none()
    );
    assert_eq!(
      detached._file.metadata().unwrap().ino() as i64,
      owner.lock_inode
    );
    drop(detached);
    let recovered = abandoned_owner(&operation, "local", directory.path())
      .unwrap()
      .unwrap();
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_none()
    );
    drop(recovered);
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_some()
    );
  }

  #[test]
  fn vm_operation_lock_rejects_missing_replaced_and_foreign_identity() {
    let directory = LockDirectory::new();
    let (owner, guard) =
      create_owner_at(uuid::Uuid::new_v4(), "local", directory.path()).unwrap();
    let operation =
      owner.operation("global.owner", uuid::Uuid::new_v4(), "updating");
    drop(guard);
    assert!(
      abandoned_owner(&operation, "foreign", directory.path())
        .unwrap()
        .is_none()
    );
    for field in ["boot", "device", "inode"] {
      let mut changed = operation.clone();
      match field {
        "boot" => changed.owner_boot_id = uuid::Uuid::new_v4().to_string(),
        "device" => changed.lock_device ^= 1,
        "inode" => changed.lock_inode ^= 1,
        _ => unreachable!(),
      }
      assert!(
        abandoned_owner(&changed, "local", directory.path())
          .unwrap()
          .is_none(),
        "{field}"
      );
    }
    let path = directory
      .0
      .join("run/vm-operations")
      .join(format!("{}.lock", owner.key));
    std::fs::rename(&path, path.with_extension("old")).unwrap();
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_none()
    );
    let (_, successor) =
      create_owner_at(owner.key, "local", directory.path()).unwrap();
    drop(successor);
    assert!(
      abandoned_owner(&operation, "local", directory.path())
        .unwrap()
        .is_none()
    );
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(path.with_extension("old"), &path).unwrap();
    assert!(abandoned_owner(&operation, "local", directory.path()).is_err());
  }

  #[test]
  fn vm_operation_lock_rejects_redirected_directories() {
    let directory = LockDirectory::new();
    let target = LockDirectory::new();
    std::os::unix::fs::symlink(&target.0, directory.0.join("run")).unwrap();
    assert!(
      create_owner_at(uuid::Uuid::new_v4(), "local", directory.path()).is_err()
    );
    assert_eq!(std::fs::read_dir(&target.0).unwrap().count(), 0);
  }

  fn test_state(
    state: &SystemState,
    node: &str,
    pool: Pool,
    docker: bollard_next::Docker,
  ) -> SystemState {
    let mut config = state.inner.config.clone();
    config.hostname = node.to_owned();
    SystemState {
      inner: Arc::new(SystemStateInner {
        pool,
        docker_api: docker,
        config,
        task_manager: Default::default(),
        event_emitter: state.inner.event_emitter.clone(),
        event_emitter_raw: Default::default(),
        arbiter: ntex::rt::Arbiter::current(),
      }),
    }
  }

  async fn accept(
    vm: &Vm,
    phase: &str,
    replacement: Option<SpecDb>,
    state: &SystemState,
  ) -> (Vm, VmOperationDb, VmOperationGuard) {
    let mutex = DistributedMutexDb::create_from(
      DistributedMutexPartial {
        action: phase.to_owned(),
        resource_kind: "Vm".into(),
        resource_key: vm.spec.vm_key.clone(),
        node_key: state.inner.config.hostname.clone(),
        acquired_by: "daemon".into(),
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
    let (owner, guard) = create_owner(mutex.key, state).unwrap();
    let operation = owner.operation(
      &vm.spec.vm_key,
      replacement.as_ref().map_or(vm.spec.key, |spec| spec.key),
      phase,
    );
    let accepted = VmDb::accept_operation(
      vm,
      replacement,
      operation.clone(),
      &state.inner.pool,
    )
    .await
    .unwrap();
    (accepted, operation, guard)
  }

  async fn create_vm(system: &crate::utils::tests::TestSystem) -> Vm {
    let response = system
      .client
      .send_post(
        "/vms",
        Some(VmSpecPartial {
          name: format!("operation-recovery-{}", uuid::Uuid::new_v4().simple()),
          image: "/tmp/operation-recovery-unused.img".into(),
          cloud_init: Some(false),
          host_config: Some(VmHostConfig {
            runtime_network: Some("bridge".into()),
            ..Default::default()
          }),
          ..Default::default()
        }),
        None::<String>,
      )
      .await;
    assert!(response.status().is_success());
    response.json::<Vm>().await.unwrap()
  }

  // Real database acceptance and Docker inspection, without starting QEMU.
  // Every scan is restricted to this fixture's unique owner node.
  #[ntex::test]
  async fn vm_recovery_committed_pending_retries_dependencies_and_preserves_successor()
   {
    use diesel::{PgConnection, r2d2::ConnectionManager};
    let system = crate::utils::tests::gen_default_test_system().await;
    let node = format!("recovery-test-{}", uuid::Uuid::new_v4());
    let state = test_state(
      &system.state,
      &node,
      system.state.inner.pool.clone(),
      system.state.inner.docker_api.clone(),
    );
    let mut vm = create_vm(&system).await;
    let key = vm.spec.vm_key.clone();
    let mut lock_keys = Vec::new();
    let result = std::panic::AssertUnwindSafe(async {
      for phase in ["starting", "updating", "destroying"] {
        let (accepted, operation, guard) =
          accept(&vm, phase, None, &state).await;
        lock_keys.push(operation.key);
        // A detached owner has no TaskManager entry yet. Removing its expired
        // mutex cannot change its kernel ownership or authorize recovery.
        DistributedMutexDb::del_by_pk(&operation.key, &state.inner.pool)
          .await
          .unwrap();
        assert!(!recover_one(&operation, &state).await.unwrap());
        assert_eq!(
          VmOperationDb::get(&key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        let unavailable_docker = test_state(
          &state,
          &node,
          state.inner.pool.clone(),
          bollard_next::Docker::connect_with_unix(
            "/dev/null",
            1,
            bollard_next::API_DEFAULT_VERSION,
          )
          .unwrap(),
        );
        let lifecycle = super::super::lifecycle_lock(&key)
          .try_lock_owned()
          .await
          .unwrap();
        let primary =
          IoError::interrupted("VM test operation", "injected primary failure");
        let primary_message = primary.to_string();
        let (result, settled) = super::super::record_task_outcome(
          &key,
          Err(primary),
          &unavailable_docker,
        )
        .await;
        assert!(!settled);
        assert_eq!(result.unwrap_err().to_string(), primary_message);
        assert_eq!(
          VmOperationDb::get(&key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status
            .actual,
          accepted.status.actual
        );
        drop(lifecycle);
        drop(guard);
        assert!(recover_one(&operation, &unavailable_docker).await.is_err());
        let unavailable_pool = Pool::builder()
          .min_idle(Some(0))
          .max_size(1)
          .connection_timeout(Duration::from_millis(50))
          .build_unchecked(ConnectionManager::<PgConnection>::new(
            "invalid-recovery-test-connection-string",
          ));
        let unavailable_db = test_state(
          &state,
          &node,
          unavailable_pool,
          state.inner.docker_api.clone(),
        );
        assert!(recover_one(&operation, &unavailable_db).await.is_err());
        assert_eq!(
          VmOperationDb::get(&key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          operation
        );
        assert_eq!(
          VmDb::transform_read_by_pk(&key, &state.inner.pool)
            .await
            .unwrap()
            .status
            .actual,
          accepted.status.actual
        );

        // The startup/background pass completes recovery without a new request.
        assert!(recover_pending(&state, None).await.unwrap().is_none());
        vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap();
        assert_eq!(vm.status.actual, ObjPsStatusKind::Stop);
        assert_eq!(vm.status.wanted, ObjPsStatusKind::Stop);
        let settled = VmOperationDb::get(&key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap();
        assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
        assert_eq!(settled.observation.as_ref().unwrap()["Runtime"], "absent");
        assert!(!recover_one(&operation, &state).await.unwrap());
        recover_pending(&state, None).await.unwrap();
        assert_eq!(
          VmOperationDb::get(&key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          settled
        );

        let (next_vm, successor, next_guard) =
          accept(&vm, "starting", None, &state).await;
        lock_keys.push(successor.key);
        assert!(!recover_one(&operation, &state).await.unwrap());
        assert_eq!(
          VmOperationDb::get(&key, &state.inner.pool)
            .await
            .unwrap()
            .unwrap(),
          successor
        );
        assert_eq!(
          DistributedMutexDb::get_lock("Vm", &key, &state.inner.pool)
            .await
            .unwrap()
            .key,
          successor.key
        );
        assert!(
          state
            .inner
            .task_manager
            .get_task(&format!("Vm@{key}"))
            .await
            .is_none()
        );
        drop(next_guard);
        assert!(recover_one(&successor, &state).await.unwrap());
        vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap();
        assert_eq!(vm.spec.key, next_vm.spec.key);
      }
    })
    .catch_unwind()
    .await;
    cleanup_mutex(&key, &state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    cleanup_locks(&lock_keys, &state);
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  async fn cleanup_mutex(key: &str, state: &SystemState) {
    if let Ok(mutex) =
      DistributedMutexDb::get_lock("Vm", key, &state.inner.pool).await
    {
      assert_eq!(mutex.node_key, state.inner.config.hostname);
      DistributedMutexDb::del_by_pk(&mutex.key, &state.inner.pool)
        .await
        .unwrap();
    }
  }

  fn cleanup_locks(keys: &[uuid::Uuid], state: &SystemState) {
    for key in keys {
      let path = std::path::Path::new(&state.inner.config.state_dir)
        .join("run/vm-operations")
        .join(format!("{key}.lock"));
      std::fs::remove_file(path).unwrap();
    }
  }

  // A disposable container models runtime identity and interruption. It is not
  // a real guest, QMP, DHCP, browser-console, or live-resize validation.
  #[ntex::test]
  async fn vm_recovery_observes_old_generation_and_partially_stopped_runtime() {
    use crate::models::{NodeDb, ObjPsStatusDb, ObjPsStatusUpdate};
    use bollard_next::container::{
      Config, CreateContainerOptions, RemoveContainerOptions,
      StartContainerOptions, StopContainerOptions,
    };
    use nanocl_stubs::generic::ImagePullPolicy;
    use std::collections::HashMap;

    const IMAGE: &str = "alpine:3.23";
    let system = crate::utils::tests::gen_default_test_system().await;
    let node = format!("runtime-recovery-test-{}", uuid::Uuid::new_v4());
    let state = test_state(
      &system.state,
      &node,
      system.state.inner.pool.clone(),
      system.state.inner.docker_api.clone(),
    );
    let mut vm = create_vm(&system).await;
    let key = vm.spec.vm_key.clone();
    let docker = &state.inner.docker_api;
    let mut runtime_id = None;
    let mut auxiliary_id = None;
    let mut lock_keys = Vec::new();
    let mut test_node_created = false;
    let result = std::panic::AssertUnwindSafe(async {
      // Process rows reference a registered node. Give this fixture its own
      // node so the real daemon scanner cannot race the controlled recovery.
      let mut test_node = NodeDb::read_by_pk(
        &system.state.inner.config.hostname,
        &state.inner.pool,
      )
      .await
      .unwrap();
      test_node.name = node.clone();
      NodeDb::create_from(test_node, &state.inner.pool)
        .await
        .unwrap();
      test_node_created = true;
      crate::utils::container::image::download(
        IMAGE,
        None,
        ImagePullPolicy::IfNotPresent,
        &vm,
        &state,
      )
      .await
      .unwrap();
      let auxiliary = docker
        .create_container(
          Some(CreateContainerOptions {
            name: format!("{key}.test.init.v"),
            ..Default::default()
          }),
          Config {
            image: Some(IMAGE.to_owned()),
            cmd: Some(vec!["sleep".to_owned(), "600".to_owned()]),
            labels: Some(HashMap::from([
              ("io.nanocl.v".to_owned(), key.clone()),
              ("io.nanocl.n".to_owned(), "global".to_owned()),
              ("io.nanocl.init-v".to_owned(), "true".to_owned()),
            ])),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      auxiliary_id = Some(auxiliary.id.clone());
      docker
        .start_container(&auxiliary.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
      let (_, provisioning, guard) =
        accept(&vm, "starting", None, &state).await;
      lock_keys.push(provisioning.key);
      drop(guard);
      assert!(
        recover_one(&provisioning, &state).await.is_err(),
        "a live init without a Process row must block retry"
      );
      assert_eq!(
        VmOperationDb::get(&key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap(),
        provisioning
      );
      docker
        .stop_container(&auxiliary.id, Some(StopContainerOptions { t: 0 }))
        .await
        .unwrap();
      assert!(recover_one(&provisioning, &state).await.unwrap());
      docker
        .remove_container(&auxiliary.id, None::<RemoveContainerOptions>)
        .await
        .unwrap();
      auxiliary_id = None;
      vm = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      let created = docker
        .create_container(
          Some(CreateContainerOptions {
            name: format!("{key}.v"),
            ..Default::default()
          }),
          Config {
            image: Some(IMAGE.to_owned()),
            cmd: Some(vec!["sleep".to_owned(), "600".to_owned()]),
            labels: Some(HashMap::from([
              ("io.nanocl.v".to_owned(), key.clone()),
              ("io.nanocl.n".to_owned(), "global".to_owned()),
              ("io.nanocl.vm-spec".to_owned(), vm.spec.key.to_string()),
            ])),
            ..Default::default()
          },
        )
        .await
        .unwrap();
      runtime_id = Some(created.id.clone());
      docker
        .start_container(&created.id, None::<StartContainerOptions<String>>)
        .await
        .unwrap();
      let inspect = docker.inspect_container(&created.id, None).await.unwrap();
      let now = chrono::Utc::now().naive_utc();
      ProcessDb::create_from(
        ProcessDb {
          key: created.id.clone(),
          name: format!("{key}.v"),
          kind: ProcessKind::Vm.to_string(),
          kind_key: key.clone(),
          node_name: node.clone(),
          data: serde_json::to_value(&inspect).unwrap(),
          created_at: now,
          updated_at: now,
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      let mut replacement = SpecDb::read_by_pk(&vm.spec.key, &state.inner.pool)
        .await
        .unwrap();
      replacement.key = uuid::Uuid::new_v4();
      replacement.data["Hostname"] = json!("interrupted-replacement");
      let (accepted, operation, guard) =
        accept(&vm, "updating", Some(replacement), &state).await;
      lock_keys.push(operation.key);
      drop(guard);
      assert!(recover_one(&operation, &state).await.unwrap());
      let current = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(current.spec.key, accepted.spec.key);
      assert_ne!(current.spec.key, vm.spec.key);
      assert_eq!(current.status.actual, ObjPsStatusKind::Start);
      assert_eq!(current.status.wanted, ObjPsStatusKind::Start);
      let settled = VmOperationDb::get(&key, &state.inner.pool)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
      let observation = settled.observation.unwrap();
      assert_eq!(observation["Specification"], json!(vm.spec.key));
      assert_eq!(observation["AcceptedGenerationObserved"], false);
      assert_eq!(observation["ContainerId"], created.id);
      assert_eq!(
        docker
          .inspect_container(&created.id, None)
          .await
          .unwrap()
          .state
          .unwrap()
          .running,
        Some(true)
      );

      let (_, destroying, guard) =
        accept(&current, "destroying", None, &state).await;
      lock_keys.push(destroying.key);
      // Interruption after runtime work has begun: it is now stopped even
      // though the persisted operation still names its pending destroy phase.
      docker
        .stop_container(&created.id, Some(StopContainerOptions { t: 0 }))
        .await
        .unwrap();
      ObjPsStatusDb::update_pk(
        &key,
        ObjPsStatusUpdate {
          actual: Some(ObjPsStatusKind::Stop.to_string()),
          ..Default::default()
        },
        &state.inner.pool,
      )
      .await
      .unwrap();
      drop(guard);
      assert!(recover_one(&destroying, &state).await.unwrap());
      let current = VmDb::transform_read_by_pk(&key, &state.inner.pool)
        .await
        .unwrap();
      assert_eq!(current.status.actual, ObjPsStatusKind::Stop);
      assert_eq!(current.status.wanted, ObjPsStatusKind::Stop);
      assert_eq!(
        VmOperationDb::get(&key, &state.inner.pool)
          .await
          .unwrap()
          .unwrap()
          .observation
          .unwrap()["Runtime"],
        "stopped"
      );
      assert!(
        docker.inspect_container(&created.id, None).await.is_ok(),
        "recovery must not replay deletion"
      );

      let (_, starting, guard) =
        accept(&current, "starting", None, &state).await;
      lock_keys.push(starting.key);
      docker
        .remove_container(&created.id, None::<RemoveContainerOptions>)
        .await
        .unwrap();
      ProcessDb::del_by_pk(&created.id, &state.inner.pool)
        .await
        .unwrap();
      runtime_id = None;
      drop(guard);
      assert!(recover_one(&starting, &state).await.unwrap());
      assert_eq!(
        VmDb::transform_read_by_pk(&key, &state.inner.pool)
          .await
          .unwrap()
          .status
          .actual,
        ObjPsStatusKind::Stop
      );
      assert!(!recover_one(&starting, &state).await.unwrap());
    })
    .catch_unwind()
    .await;

    if let Some(id) = auxiliary_id {
      docker
        .remove_container(
          &id,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await
        .unwrap();
    }
    if let Some(id) = runtime_id {
      docker
        .remove_container(
          &id,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await
        .unwrap();
      ProcessDb::del_by_pk(&id, &state.inner.pool).await.unwrap();
    }
    cleanup_mutex(&key, &state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    cleanup_locks(&lock_keys, &state);
    if test_node_created {
      NodeDb::del_by_pk(&node, &state.inner.pool).await.unwrap();
    }
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }

  // Unlike the isolated-node tests above, this fixture deliberately uses the
  // existing development/CI daemon's node. No test scanner or recovery helper
  // is started: only that separate daemon can settle this committed operation.
  #[ntex::test]
  async fn vm_recovery_external_daemon_scans_commit_before_registration() {
    let system = crate::utils::tests::gen_default_test_system().await;
    let state = &system.state;
    let state_root = std::fs::canonicalize(&state.inner.config.state_dir)
      .expect("the daemon and test runner must share their state directory");
    let mut scanner_daemon = None;
    // The normal development Compose and prepare_test_ci.sh use these names.
    // Inspect deployment metadata first so a missing external daemon is a
    // precise prerequisite failure, not a silently skipped recovery test.
    for name in ["system.ndaemon.c", "nanocld-ci"] {
      let daemon =
        match state.inner.docker_api.inspect_container(name, None).await {
          Ok(daemon) => daemon,
          Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          }) => continue,
          Err(error) => {
            panic!("cannot inspect external recovery daemon {name}: {error}")
          }
        };
      if daemon.state.as_ref().and_then(|state| state.running) != Some(true)
        || daemon
          .config
          .as_ref()
          .and_then(|config| config.hostname.as_deref())
          != Some(state.inner.config.hostname.as_str())
      {
        continue;
      }
      let shared_state = daemon
        .mounts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|mount| {
          let (Some(source), Some(destination)) =
            (&mount.source, &mount.destination)
          else {
            return false;
          };
          if mount.rw != Some(true) {
            return false;
          }
          let Ok(suffix) = std::path::Path::new(&state.inner.config.state_dir)
            .strip_prefix(destination)
          else {
            return false;
          };
          std::fs::canonicalize(std::path::Path::new(source).join(suffix))
            .is_ok_and(|mapped| mapped == state_root)
        });
      if shared_state {
        assert!(
          scanner_daemon.is_none(),
          "multiple external daemons match the test node and state directory"
        );
        scanner_daemon = Some(name);
      }
    }
    let scanner_daemon = scanner_daemon.expect(
      "start the current nanocld through development Compose or prepare_test_ci.sh with the test node and shared state directory before running this regression",
    );
    let vm = create_vm(&system).await;
    let key = vm.spec.vm_key.clone();
    let mut lock_keys = Vec::new();
    let result = std::panic::AssertUnwindSafe(async {
      let (accepted, operation, guard) = accept(&vm, "starting", None, state).await;
      lock_keys.push(operation.key);
      assert_eq!(VmOperationDb::get(&key, &state.inner.pool).await.unwrap().unwrap(), operation);
      assert!(state.inner.task_manager.get_task(&format!("Vm@{key}")).await.is_none());
      // Acceptance is committed, but no executor was registered or polled.
      // Releasing the last kernel owner models its interruption at that gap.
      drop(guard);
      let settled = tokio::time::timeout(Duration::from_secs(45), async {
        let mut observations = tokio::time::interval(Duration::from_millis(100));
        loop {
          observations.tick().await;
          let current = VmOperationDb::get(&key, &state.inner.pool).await.unwrap().unwrap();
          assert_eq!(current.key, operation.key);
          assert_eq!(current.spec_key, accepted.spec.key);
          if current.outcome.is_some() { break current; }
        }
      }).await.unwrap_or_else(|_| panic!("external daemon {scanner_daemon} did not recover the committed operation within 45 seconds; verify it runs the current source and shares the owner file"));
      assert_eq!(settled.outcome.as_deref(), Some("interrupted"));
      assert_eq!(settled.observation.as_ref().unwrap()["Runtime"], "absent");
      let observed = VmDb::transform_read_by_pk(&key, &state.inner.pool).await.unwrap();
      assert_eq!(observed.spec.key, accepted.spec.key);
      assert_eq!(observed.status.actual, ObjPsStatusKind::Stop);
      assert_eq!(observed.status.wanted, ObjPsStatusKind::Stop);
      assert!(matches!(state.inner.docker_api.inspect_container(&format!("{key}.v"), None).await,
        Err(bollard_next::errors::Error::DockerResponseServerError { status_code: 404, .. })));
      assert!(state.inner.task_manager.get_task(&format!("Vm@{key}")).await.is_none());
      assert_eq!(VmOperationDb::get(&key, &state.inner.pool).await.unwrap().unwrap(), settled);
    }).catch_unwind().await;
    cleanup_mutex(&key, state).await;
    VmDb::clear_by_pk(&key, &state.inner.pool).await.unwrap();
    cleanup_locks(&lock_keys, state);
    if let Err(panic) = result {
      std::panic::resume_unwind(panic);
    }
  }
}
