use std::{
  ffi::CString,
  fs::{File, OpenOptions},
  io::{Read, Write},
  os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
  },
  sync::Arc,
};

use nanocl_error::io::{IoError, IoResult};

use crate::models::{
  OperationCleanupCursor, OperationDb, OperationGuard, OperationOwner,
  OperationOwnerFile, Pool, StateDirectoryGuard,
};

const OWNER_FILE_VERSION: u8 = 1;
const MAX_OWNER_FILE_BYTES: u64 = 4096;
const CLEANUP_BATCH_SIZE: usize = 32;

fn evidence_error(message: &str) -> IoError {
  IoError::invalid_data("Operation ownership", message)
}

fn busy(message: &str) -> IoError {
  IoError::with_context(
    "Operation ownership",
    std::io::Error::new(std::io::ErrorKind::WouldBlock, message),
  )
}

fn boot_id() -> IoResult<String> {
  let value = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
  let value = value.trim();
  uuid::Uuid::parse_str(value)
    .map_err(|_| evidence_error("Linux boot identity is invalid"))?;
  Ok(value.to_owned())
}

fn open_directory(parent: &File, name: &str, create: bool) -> IoResult<File> {
  let name = CString::new(name).unwrap();
  if create {
    let result =
      unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
    if result != 0 {
      let error = std::io::Error::last_os_error();
      if error.kind() != std::io::ErrorKind::AlreadyExists {
        return Err(error.into());
      }
    }
    parent.sync_all()?;
  }
  let fd = unsafe {
    libc::openat(
      parent.as_raw_fd(),
      name.as_ptr(),
      libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
    )
  };
  if fd < 0 {
    return Err(std::io::Error::last_os_error().into());
  }
  Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_regular(parent: &File, name: &str, flags: i32) -> IoResult<File> {
  let name = CString::new(name).unwrap();
  let fd = unsafe {
    libc::openat(
      parent.as_raw_fd(),
      name.as_ptr(),
      libc::O_RDWR
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | flags,
      0o600,
    )
  };
  if fd < 0 {
    return Err(std::io::Error::last_os_error().into());
  }
  let file = unsafe { File::from_raw_fd(fd) };
  let metadata = file.metadata()?;
  if !metadata.is_file() || metadata.nlink() != 1 {
    return Err(evidence_error(
      "Ownership evidence must be a regular file with one link; inspect the state directory while all controllers are stopped",
    ));
  }
  Ok(file)
}

fn try_lock(file: &File, exclusive: bool) -> IoResult<bool> {
  let kind = if exclusive {
    libc::LOCK_EX
  } else {
    libc::LOCK_SH
  };
  if unsafe { libc::flock(file.as_raw_fd(), kind | libc::LOCK_NB) } == 0 {
    return Ok(true);
  }
  let error = std::io::Error::last_os_error();
  if error.kind() == std::io::ErrorKind::WouldBlock {
    return Ok(false);
  }
  Err(error.into())
}

fn acquire_gate(
  state_dir: &str,
  exclusive: bool,
  create_root: bool,
) -> IoResult<StateDirectoryGuard> {
  if create_root {
    std::fs::create_dir_all(state_dir)?;
  }
  // Aliases of the trusted configured root are allowed. Descendants are opened
  // relative to pinned descriptors, without following symlinks.
  let state_dir = std::fs::canonicalize(state_dir)?;
  let root = OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(&state_dir)?;
  let run = open_directory(&root, "run", true)?;
  let file = open_regular(&run, "operations.gate", libc::O_CREAT)?;
  if !try_lock(&file, exclusive)? {
    return Err(busy(if exclusive {
      "A daemon or another maintenance command still holds the state directory; stop all controllers before reconciliation"
    } else {
      "Exclusive operation reconciliation is in progress for this state directory"
    }));
  }
  file.sync_all()?;
  run.sync_all()?;
  Ok(StateDirectoryGuard {
    _file: Arc::new(file),
    root: Arc::new(root),
    exclusive,
  })
}

pub(crate) fn acquire_daemon_guard(
  state_dir: &str,
) -> IoResult<StateDirectoryGuard> {
  acquire_gate(state_dir, false, true)
}

pub(crate) fn acquire_inspection_guard(
  state_dir: &str,
) -> IoResult<StateDirectoryGuard> {
  acquire_gate(state_dir, false, false)
}

/// Existing state only. The exclusive gate supplements, rather than replaces,
/// the operator's quiescence prerequisite for older or foreign-state daemons.
pub(crate) fn acquire_reconciliation_guard(
  state_dir: &str,
) -> IoResult<StateDirectoryGuard> {
  acquire_gate(state_dir, true, false)
}

fn operation_directory(
  gate: &StateDirectoryGuard,
  create: bool,
) -> IoResult<File> {
  let run = open_directory(&gate.root, "run", create)?;
  open_directory(&run, "operations", create)
}

fn filename(key: uuid::Uuid) -> String {
  format!("{key}.lock")
}

fn read_owner(file: &File, key: uuid::Uuid) -> IoResult<OperationOwner> {
  if file.metadata()?.len() > MAX_OWNER_FILE_BYTES {
    return Err(evidence_error(
      "Ownership file exceeds the bounded payload size",
    ));
  }
  let mut bytes = Vec::new();
  file
    .take(MAX_OWNER_FILE_BYTES + 1)
    .read_to_end(&mut bytes)?;
  if bytes.len() as u64 > MAX_OWNER_FILE_BYTES {
    return Err(evidence_error(
      "Ownership file exceeds the bounded payload size",
    ));
  }
  let payload: OperationOwnerFile = serde_json::from_slice(&bytes).map_err(|_| {
    evidence_error("Ownership file is incomplete or malformed; use quiescent operator reconciliation")
  })?;
  if payload.version != OWNER_FILE_VERSION
    || payload.owner.key != key
    || payload.owner.resource_kind.is_empty()
    || payload.owner.resource_key.is_empty()
    || payload.owner.node_name.is_empty()
    || uuid::Uuid::parse_str(&payload.owner.owner_boot_id).is_err()
  {
    return Err(evidence_error(
      "Ownership file identity is invalid or unsupported",
    ));
  }
  Ok(payload.owner)
}

fn file_matches(file: &File, owner: &OperationOwner) -> IoResult<bool> {
  let metadata = file.metadata()?;
  Ok(
    metadata.is_file()
      && metadata.nlink() == 1
      && metadata.dev() as i64 == owner.lock_device
      && metadata.ino() as i64 == owner.lock_inode,
  )
}

pub(crate) fn create_owner(
  key: uuid::Uuid,
  resource_kind: &str,
  resource_key: &str,
  incarnation_key: uuid::Uuid,
  node_name: &str,
  state_dir: &str,
) -> IoResult<(OperationOwner, OperationGuard)> {
  create_owner_with_gate(
    key,
    resource_kind,
    resource_key,
    incarnation_key,
    node_name,
    acquire_daemon_guard(state_dir)?,
  )
}

/// The incarnation must already be committed before this is called. This lets
/// maintenance fence an uncertain acceptance using that same retained row.
pub(crate) fn create_owner_with_gate(
  key: uuid::Uuid,
  resource_kind: &str,
  resource_key: &str,
  incarnation_key: uuid::Uuid,
  node_name: &str,
  gate: StateDirectoryGuard,
) -> IoResult<(OperationOwner, OperationGuard)> {
  if resource_kind.is_empty() || resource_key.is_empty() || node_name.is_empty()
  {
    return Err(evidence_error(
      "Ownership resource and node identities must be nonempty",
    ));
  }
  let owner_boot_id = boot_id()?;
  let directory = Arc::new(operation_directory(&gate, true)?);
  let mut file =
    open_regular(&directory, &filename(key), libc::O_CREAT | libc::O_EXCL)?;
  if !try_lock(&file, true)? {
    return Err(busy(
      "New ownership file is locked; acceptance has not started",
    ));
  }
  let metadata = file.metadata()?;
  let owner = OperationOwner {
    key,
    resource_kind: resource_kind.to_owned(),
    resource_key: resource_key.to_owned(),
    incarnation_key,
    node_name: node_name.to_owned(),
    owner_boot_id,
    lock_device: metadata.dev() as i64,
    lock_inode: metadata.ino() as i64,
  };
  let written = (|| {
    let bytes = serde_json::to_vec(&OperationOwnerFile {
      version: OWNER_FILE_VERSION,
      owner: owner.clone(),
    })
    .map_err(|error| evidence_error(&error.to_string()))?;
    if bytes.len() as u64 > MAX_OWNER_FILE_BYTES {
      return Err(evidence_error(
        "Ownership identity exceeds the bounded payload size",
      ));
    }
    file.write_all(&bytes)?;
    file.sync_all()?;
    directory.sync_all()?;
    Ok(())
  })();
  let guard = OperationGuard {
    _file: Arc::new(file),
    directory,
    owner: owner.clone(),
    identity_verified: true,
    _gate: gate,
  };
  if let Err(error) = written {
    // Creation has not returned, so no acceptance worker can have been given
    // this token. This is a definitive pre-acceptance rejection.
    if let Err(cleanup) = unlink_exact(&guard) {
      log::warn!("Incomplete ownership-file cleanup deferred: {cleanup}");
    }
    return Err(error);
  }
  Ok((owner, guard))
}

pub(crate) fn abandoned_owner(
  owner: &OperationOwner,
  node_name: &str,
  state_dir: &str,
) -> IoResult<Option<OperationGuard>> {
  if owner.node_name != node_name {
    return Err(evidence_error(
      "Owner belongs to another node; reconcile on the owning node after confirming controller quiescence",
    ));
  }
  if owner.owner_boot_id != boot_id()? {
    return Err(evidence_error(
      "Owner boot differs; automatic takeover is not authorized; use quiescent operator reconciliation",
    ));
  }
  let gate = acquire_daemon_guard(state_dir)?;
  let directory = Arc::new(operation_directory(&gate, false).map_err(|error| {
    evidence_error(&format!("Ownership directory unavailable: {error}; use quiescent operator reconciliation"))
  })?);
  let file = open_regular(&directory, &filename(owner.key), 0).map_err(|error| {
    evidence_error(&format!("Ownership file unavailable: {error}; use quiescent operator reconciliation"))
  })?;
  if !try_lock(&file, true)? {
    return Ok(None);
  }
  if !file_matches(&file, owner)? || read_owner(&file, owner.key)? != *owner {
    return Err(evidence_error(
      "Ownership file was replaced or does not match the accepted token; use quiescent operator reconciliation",
    ));
  }
  Ok(Some(OperationGuard {
    _file: Arc::new(file),
    directory,
    owner: owner.clone(),
    identity_verified: true,
    _gate: gate,
  }))
}

/// Offline reconciliation derives its authority from the exclusive gate and
/// explicit operator quiescence, not from missing/replaced/old-boot evidence.
/// Even then an existing busy file is an absolute refusal. Free ambiguous files
/// remain pinned but are ineligible for automatic unlinking.
pub(crate) fn reconciliation_owner(
  owner: &OperationOwner,
  node_name: &str,
  gate: &StateDirectoryGuard,
) -> IoResult<Option<OperationGuard>> {
  if !gate.exclusive {
    return Err(evidence_error(
      "Reconciliation requires the exclusive state-directory gate",
    ));
  }
  if owner.node_name != node_name {
    return Err(evidence_error(
      "Reconcile with the recorded owner node configuration and explicit controller quiescence",
    ));
  }
  let directory = match operation_directory(gate, false) {
    Ok(directory) => Arc::new(directory),
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      return Ok(None);
    }
    Err(error) => return Err(error),
  };
  let file = match open_regular(&directory, &filename(owner.key), 0) {
    Ok(file) => file,
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      return Ok(None);
    }
    Err(error) => {
      return Err(evidence_error(&format!(
        "Unsafe ownership path: {error}; inspect it with all controllers stopped"
      )));
    }
  };
  if !try_lock(&file, true)? {
    return Err(busy(
      "Ownership file is still locked by a live process; reconciliation is refused",
    ));
  }
  let identity_verified = file_matches(&file, owner)?
    && read_owner(&file, owner.key).is_ok_and(|found| found == *owner);
  Ok(Some(OperationGuard {
    _file: Arc::new(file),
    directory,
    owner: owner.clone(),
    identity_verified,
    _gate: gate.clone(),
  }))
}

fn unlink_exact(guard: &OperationGuard) -> IoResult<bool> {
  if !guard.identity_verified
    || Arc::strong_count(&guard._file) != 1
    || !file_matches(&guard._file, &guard.owner)?
  {
    return Ok(false);
  }
  let name = CString::new(filename(guard.owner.key)).unwrap();
  let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
  if unsafe {
    libc::fstatat(
      guard.directory.as_raw_fd(),
      name.as_ptr(),
      metadata.as_mut_ptr(),
      libc::AT_SYMLINK_NOFOLLOW,
    )
  } != 0
  {
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::NotFound {
      return Ok(false);
    }
    return Err(error.into());
  }
  let metadata = unsafe { metadata.assume_init() };
  if metadata.st_dev as i64 != guard.owner.lock_device
    || metadata.st_ino as i64 != guard.owner.lock_inode
    || metadata.st_mode & libc::S_IFMT != libc::S_IFREG
    || metadata.st_nlink != 1
  {
    return Ok(false);
  }
  if unsafe { libc::unlinkat(guard.directory.as_raw_fd(), name.as_ptr(), 0) }
    != 0
  {
    return Err(std::io::Error::last_os_error().into());
  }
  guard.directory.sync_all()?;
  Ok(true)
}

/// Used after completion or rejection alike. A failed/unknown commit does not
/// authorize deletion: the repository first fences that incarnation's writer
/// and verifies a terminal exact token or a definitively absent operation.
pub(crate) async fn cleanup_owner(
  guard: &OperationGuard,
  pool: &Pool,
) -> IoResult<bool> {
  // A direct caller may still share its lease with a queued blocking worker
  // which has not acquired the incarnation row yet. Leave the evidence until
  // every such worker exits; maintenance then obtains a separate free flock.
  if !guard.identity_verified || Arc::strong_count(&guard._file) != 1 {
    return Ok(false);
  }
  if !OperationDb::owner_file_cleanup_eligible(
    guard.owner.clone(),
    guard.clone(),
    pool,
  )
  .await?
  {
    return Ok(false);
  }
  unlink_exact(guard)
}

fn cleanup_candidates(
  cursor: &mut OperationCleanupCursor,
  node_name: &str,
) -> IoResult<(Vec<OperationGuard>, bool)> {
  let mut candidates = Vec::new();
  for _ in 0..CLEANUP_BATCH_SIZE {
    let Some(entry) = cursor.entries.next() else {
      return Ok((candidates, true));
    };
    let entry = entry?;
    let name = entry.file_name();
    let Some(key) = name
      .to_str()
      .and_then(|name| name.strip_suffix(".lock"))
      .and_then(|name| uuid::Uuid::parse_str(name).ok())
    else {
      continue;
    };
    let candidate = (|| -> IoResult<Option<OperationGuard>> {
      let file = open_regular(&cursor.directory, &filename(key), 0)?;
      if !try_lock(&file, true)? {
        return Ok(None);
      }
      let owner = read_owner(&file, key)?;
      if owner.node_name != node_name || !file_matches(&file, &owner)? {
        return Ok(None);
      }
      Ok(Some(OperationGuard {
        _file: Arc::new(file),
        directory: cursor.directory.clone(),
        owner,
        identity_verified: true,
        _gate: cursor.gate.clone(),
      }))
    })();
    match candidate {
      Ok(Some(guard)) => candidates.push(guard),
      Ok(None) => {}
      Err(error) => {
        log::warn!("Operation {key} ownership cleanup deferred: {error}")
      }
    }
  }
  Ok((candidates, false))
}

pub(crate) async fn maintenance_step(
  state_dir: &str,
  node_name: &str,
  cursor: &mut Option<OperationCleanupCursor>,
  pool: &Pool,
) -> IoResult<usize> {
  if cursor.is_none() {
    let gate = acquire_daemon_guard(state_dir)?;
    let directory = match operation_directory(&gate, false) {
      Ok(directory) => Arc::new(directory),
      Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
        return Ok(0);
      }
      Err(error) => return Err(error),
    };
    // This proc descriptor path names the already-pinned directory. Entry paths
    // from ReadDir are never trusted: every candidate is opened using openat.
    let entries =
      std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?;
    *cursor = Some(OperationCleanupCursor {
      directory,
      entries,
      gate,
    });
  }
  let (candidates, exhausted) =
    cleanup_candidates(cursor.as_mut().unwrap(), node_name)?;
  if exhausted {
    *cursor = None;
  }
  let mut removed = 0;
  for guard in candidates {
    match cleanup_owner(&guard, pool).await {
      Ok(true) => removed += 1,
      Ok(false) => {}
      Err(error) => log::warn!(
        "Operation {} ownership cleanup deferred: {error}",
        guard.owner.key
      ),
    }
  }
  Ok(removed)
}

#[cfg(test)]
mod tests {
  use std::{
    collections::HashSet,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
  };

  use super::*;

  struct StateDirectory(PathBuf);

  impl StateDirectory {
    fn new() -> Self {
      let path = std::env::temp_dir()
        .join(format!("nanocl-operation-{}", uuid::Uuid::new_v4()));
      std::fs::create_dir(&path).unwrap();
      Self(path)
    }

    fn path(&self) -> &str {
      self.0.to_str().unwrap()
    }

    fn owner(&self) -> (OperationOwner, OperationGuard) {
      create_owner(
        uuid::Uuid::new_v4(),
        "Vm",
        "global.owner-test",
        uuid::Uuid::new_v4(),
        "owner-test-node",
        self.path(),
      )
      .unwrap()
    }

    fn lock_path(&self, owner: &OperationOwner) -> PathBuf {
      self.0.join("run/operations").join(filename(owner.key))
    }

    fn cursor(&self) -> OperationCleanupCursor {
      let gate = acquire_daemon_guard(self.path()).unwrap();
      let directory = Arc::new(operation_directory(&gate, false).unwrap());
      let entries =
        std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))
          .unwrap();
      OperationCleanupCursor {
        directory,
        entries,
        gate,
      }
    }
  }

  impl Drop for StateDirectory {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  struct ChildOwner(Child);

  impl Drop for ChildOwner {
    fn drop(&mut self) {
      let _ = self.0.kill();
      let _ = self.0.wait();
    }
  }

  #[test]
  #[ignore = "subprocess helper invoked by process_owner_is_live_until_exit"]
  fn operation_lock_child() {
    let path = std::env::var("NANOCL_OPERATION_LOCK_TEST_STATE").unwrap();
    let (owner, _guard) = create_owner(
      uuid::Uuid::new_v4(),
      "Vm",
      "global.child-owner-test",
      uuid::Uuid::new_v4(),
      "owner-test-node",
      &path,
    )
    .unwrap();
    println!("OPERATION_OWNER:{}", serde_json::to_string(&owner).unwrap());
    std::io::stdout().flush().unwrap();
    let mut finish = [0];
    let _ = std::io::stdin().read_exact(&mut finish);
  }

  #[test]
  fn process_owner_is_live_until_exit() {
    let state = StateDirectory::new();
    let child = Command::new(std::env::current_exe().unwrap())
      .args([
        "--exact",
        "utils::operation::tests::operation_lock_child",
        "--ignored",
        "--nocapture",
      ])
      .env("NANOCL_OPERATION_LOCK_TEST_STATE", state.path())
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()
      .unwrap();
    let mut child = ChildOwner(child);
    let output = child.0.stdout.take().unwrap();
    let (ready, received) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
      for line in BufReader::new(output).lines() {
        let line = line.unwrap();
        if let Some((_, payload)) = line.split_once("OPERATION_OWNER:") {
          ready
            .send(serde_json::from_str::<OperationOwner>(payload).unwrap())
            .unwrap();
          return;
        }
      }
    });
    let owner = received
      .recv_timeout(Duration::from_secs(30))
      .expect("child did not publish its held ownership lock");
    reader.join().unwrap();
    assert!(
      abandoned_owner(&owner, "owner-test-node", state.path())
        .unwrap()
        .is_none()
    );
    assert!(acquire_reconciliation_guard(state.path()).is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let recovered = abandoned_owner(&owner, "owner-test-node", state.path())
      .unwrap()
      .expect("process exit must release the kernel lock");
    assert_eq!(recovered.owner, owner);
  }

  #[test]
  fn worker_clone_retains_owner_and_gate_until_actual_exit() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    let worker_guard = guard.clone();
    let (finish, received) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
      let _guard = worker_guard;
      received.recv().unwrap();
    });
    drop(guard);
    assert!(
      abandoned_owner(&owner, "owner-test-node", state.path())
        .unwrap()
        .is_none()
    );
    assert!(acquire_reconciliation_guard(state.path()).is_err());
    finish.send(()).unwrap();
    worker.join().unwrap();
    let recovered = abandoned_owner(&owner, "owner-test-node", state.path())
      .unwrap()
      .unwrap();
    assert!(acquire_reconciliation_guard(state.path()).is_err());
    drop(recovered);
    assert!(acquire_reconciliation_guard(state.path()).is_ok());
  }

  #[test]
  fn ambiguous_owner_evidence_never_authorizes_automatic_takeover() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    drop(guard);
    assert!(abandoned_owner(&owner, "other-node", state.path()).is_err());
    let mut old_boot = owner.clone();
    old_boot.owner_boot_id = uuid::Uuid::new_v4().to_string();
    assert!(
      abandoned_owner(&old_boot, "owner-test-node", state.path()).is_err()
    );
    let mut wrong_inode = owner.clone();
    wrong_inode.lock_inode = wrong_inode.lock_inode.wrapping_add(1);
    assert!(
      abandoned_owner(&wrong_inode, "owner-test-node", state.path()).is_err()
    );
    let mut wrong_incarnation = owner.clone();
    wrong_incarnation.incarnation_key = uuid::Uuid::new_v4();
    assert!(
      abandoned_owner(&wrong_incarnation, "owner-test-node", state.path())
        .is_err()
    );
    let path = state.lock_path(&owner);
    std::fs::rename(&path, path.with_extension("old")).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
  }

  #[test]
  fn ownership_paths_reject_redirected_directories_and_files() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    drop(guard);
    let directory = state.0.join("run/operations");
    let moved = state.0.join("run/operations-original");
    std::fs::rename(&directory, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &directory).unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
    std::fs::remove_file(&directory).unwrap();
    std::fs::rename(&moved, &directory).unwrap();
    let path = state.lock_path(&owner);
    std::fs::rename(&path, path.with_extension("old")).unwrap();
    std::os::unix::fs::symlink(path.with_extension("old"), &path).unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
    let run = state.0.join("run");
    let moved = state.0.join("run-original");
    std::fs::rename(&run, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &run).unwrap();
    assert!(acquire_daemon_guard(state.path()).is_err());
    assert!(acquire_reconciliation_guard(state.path()).is_err());
  }

  #[test]
  fn offline_reconciliation_requires_exclusivity_and_preserves_ambiguous_files()
  {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    assert!(
      reconciliation_owner(&owner, "owner-test-node", &guard._gate).is_err()
    );
    drop(guard);
    let gate = acquire_reconciliation_guard(state.path()).unwrap();
    assert!(acquire_daemon_guard(state.path()).is_err());
    let exact = reconciliation_owner(&owner, "owner-test-node", &gate)
      .unwrap()
      .unwrap();
    assert!(exact.identity_verified);
    assert!(reconciliation_owner(&owner, "owner-test-node", &gate).is_err());
    drop(exact);
    let path = state.lock_path(&owner);
    std::fs::write(&path, b"incomplete evidence").unwrap();
    let ambiguous = reconciliation_owner(&owner, "owner-test-node", &gate)
      .unwrap()
      .unwrap();
    assert!(!ambiguous.identity_verified);
    assert!(!unlink_exact(&ambiguous).unwrap());
    assert!(path.exists());
    drop(ambiguous);
    std::fs::remove_file(&path).unwrap();
    assert!(
      reconciliation_owner(&owner, "owner-test-node", &gate)
        .unwrap()
        .is_none()
    );
    std::os::unix::fs::symlink(Path::new("missing-evidence"), &path).unwrap();
    assert!(reconciliation_owner(&owner, "owner-test-node", &gate).is_err());
  }

  #[test]
  fn exact_cleanup_preserves_replacement_and_stable_gate() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    let path = state.lock_path(&owner);
    let worker = guard.clone();
    assert!(!unlink_exact(&guard).unwrap());
    assert!(path.exists());
    drop(worker);
    let moved = path.with_extension("old");
    std::fs::rename(&path, &moved).unwrap();
    std::fs::write(&path, b"replacement must survive").unwrap();
    assert!(!unlink_exact(&guard).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement must survive");
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&moved, &path).unwrap();
    assert!(unlink_exact(&guard).unwrap());
    assert!(!path.exists());
    assert!(!unlink_exact(&guard).unwrap());
    assert!(state.0.join("run/operations.gate").exists());
  }

  #[test]
  fn inspection_requires_existing_state_and_blocks_exclusive_reconciliation() {
    let state = StateDirectory::new();
    let missing = state.0.join("missing");
    assert!(acquire_inspection_guard(missing.to_str().unwrap()).is_err());
    assert!(!missing.exists());
    let inspection = acquire_inspection_guard(state.path()).unwrap();
    assert!(acquire_reconciliation_guard(state.path()).is_err());
    drop(inspection);
    assert!(acquire_reconciliation_guard(state.path()).is_ok());
  }

  #[test]
  fn rejected_owner_creation_removes_its_unpublished_file() {
    let state = StateDirectory::new();
    let key = uuid::Uuid::new_v4();
    assert!(
      create_owner(
        key,
        "Vm",
        &"x".repeat(MAX_OWNER_FILE_BYTES as usize),
        uuid::Uuid::new_v4(),
        "owner-test-node",
        state.path()
      )
      .is_err()
    );
    assert!(!state.0.join("run/operations").join(filename(key)).exists());
    assert_eq!(
      std::fs::read_dir(state.0.join("run/operations"))
        .unwrap()
        .count(),
      0
    );
    assert!(state.0.join("run/operations.gate").exists());
  }

  #[test]
  fn bounded_cleanup_cursor_progresses_past_live_and_unknown_files() {
    let state = StateDirectory::new();
    let (live_owner, live_guard) = state.owner();
    let foreign_key = uuid::Uuid::new_v4();
    let (_, foreign_guard) = create_owner(
      foreign_key,
      "Vm",
      "global.foreign-test",
      uuid::Uuid::new_v4(),
      "foreign-node",
      state.path(),
    )
    .unwrap();
    drop(foreign_guard);
    let malformed = state
      .0
      .join("run/operations")
      .join(filename(uuid::Uuid::new_v4()));
    std::fs::write(&malformed, b"incomplete evidence").unwrap();
    let mut expected = HashSet::new();
    for _ in 0..(CLEANUP_BATCH_SIZE * 2 + 5) {
      let (owner, guard) = state.owner();
      expected.insert(owner.key);
      drop(guard);
    }
    let mut cursor = state.cursor();
    let mut found = HashSet::new();
    let mut exhausted = false;
    for _ in 0..4 {
      let (candidates, finished) =
        cleanup_candidates(&mut cursor, "owner-test-node").unwrap();
      assert!(candidates.len() <= CLEANUP_BATCH_SIZE);
      for candidate in candidates {
        assert!(
          found.insert(candidate.owner.key),
          "cursor revisited an entry"
        );
      }
      if finished {
        exhausted = true;
        break;
      }
    }
    assert!(
      exhausted,
      "bounded cursor did not make progress across passes"
    );
    assert_eq!(found, expected);
    assert!(malformed.exists());
    assert!(
      state
        .0
        .join("run/operations")
        .join(filename(foreign_key))
        .exists()
    );
    assert!(
      abandoned_owner(&live_owner, "owner-test-node", state.path())
        .unwrap()
        .is_none()
    );
    drop(live_guard);
  }
}
