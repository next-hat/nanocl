use std::{
  ffi::CString,
  fs::{File, OpenOptions},
  io::{Read, Write},
  os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::{MetadataExt, OpenOptionsExt},
  },
  sync::{Arc, OnceLock},
};

use nanocl_error::io::{IoError, IoResult};

use crate::models::{
  OperationCleanupCursor, OperationDb, OperationGuard, OperationOwner,
  OperationOwnerFile, Pool, StateDirectoryGuard, StateDirectoryLock,
  StateDirectoryLocks,
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
      "Ownership evidence must be a regular file with one link",
    ));
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

/// One controller owns the persistent installation. Repeated acquisitions in
/// this process share the same open file description, including DB workers.
pub(crate) fn acquire_daemon_guard(
  state_dir: &str,
) -> IoResult<StateDirectoryGuard> {
  static LOCKS: OnceLock<StateDirectoryLocks> = OnceLock::new();
  std::fs::create_dir_all(state_dir)?;
  // Root aliases are permitted; descendants are pinned and never followed.
  let state_dir = std::fs::canonicalize(state_dir)?;
  let mut locks = LOCKS
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|error| error.into_inner());
  locks.retain(|_, guard| guard.strong_count() > 0);
  if let Some(guard) = locks.get(&state_dir).and_then(std::sync::Weak::upgrade)
  {
    return Ok(guard);
  }
  let root = OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(&state_dir)?;
  let run = open_directory(&root, "run", true)?;
  let file = open_regular(&run, "operations.gate", libc::O_CREAT)?;
  if !try_lock(&file)? {
    return Err(busy(
      "Another daemon controller owns this persistent state directory",
    ));
  }
  file.sync_all()?;
  run.sync_all()?;
  let guard = Arc::new(StateDirectoryLock { _file: file, root });
  locks.insert(state_dir, Arc::downgrade(&guard));
  Ok(guard)
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
    evidence_error("Ownership file is incomplete or malformed; automatic recovery is deferred")
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

fn file_matches(file: &File, device: i64, inode: i64) -> IoResult<bool> {
  let metadata = file.metadata()?;
  Ok(
    metadata.is_file()
      && metadata.nlink() == 1
      && metadata.dev() as i64 == device
      && metadata.ino() as i64 == inode,
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
  create_owner_at_boot(
    key,
    resource_kind,
    resource_key,
    incarnation_key,
    node_name,
    gate,
    boot_id()?,
  )
}

/// Simulates previously persisted ownership without changing the host boot ID.
#[cfg(test)]
pub(crate) fn create_owner_with_boot_id(
  key: uuid::Uuid,
  resource_kind: &str,
  resource_key: &str,
  incarnation_key: uuid::Uuid,
  node_name: &str,
  state_dir: &str,
  owner_boot_id: &str,
) -> IoResult<(OperationOwner, OperationGuard)> {
  uuid::Uuid::parse_str(owner_boot_id)
    .map_err(|_| evidence_error("Simulated boot identity is invalid"))?;
  create_owner_at_boot(
    key,
    resource_kind,
    resource_key,
    incarnation_key,
    node_name,
    acquire_daemon_guard(state_dir)?,
    owner_boot_id.to_owned(),
  )
}

fn create_owner_at_boot(
  key: uuid::Uuid,
  resource_kind: &str,
  resource_key: &str,
  incarnation_key: uuid::Uuid,
  node_name: &str,
  gate: StateDirectoryGuard,
  owner_boot_id: String,
) -> IoResult<(OperationOwner, OperationGuard)> {
  if resource_kind.is_empty() || resource_key.is_empty() || node_name.is_empty()
  {
    return Err(evidence_error(
      "Ownership resource and node identities must be nonempty",
    ));
  }
  let directory = Arc::new(operation_directory(&gate, true)?);
  let mut file =
    open_regular(&directory, &filename(key), libc::O_CREAT | libc::O_EXCL)?;
  if !try_lock(&file)? {
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
    file_device: metadata.dev() as i64,
    file_inode: metadata.ino() as i64,
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
      "Owner belongs to another node; automatic recovery is deferred",
    ));
  }
  let gate = acquire_daemon_guard(state_dir)?;
  let directory = Arc::new(operation_directory(&gate, false).map_err(|error| {
    evidence_error(&format!("Ownership directory unavailable: {error}; automatic recovery is deferred"))
  })?);
  let file =
    open_regular(&directory, &filename(owner.key), 0).map_err(|error| {
      evidence_error(&format!(
        "Ownership file unavailable: {error}; automatic recovery is deferred"
      ))
    })?;
  if !try_lock(&file)? {
    return Ok(None);
  }
  // The controller gate proves installation ownership across a restart. A
  // remount may change filesystem numbers; the immutable saved token may not.
  if read_owner(&file, owner.key)? != *owner
    || (owner.owner_boot_id == boot_id()?
      && !file_matches(&file, owner.lock_device, owner.lock_inode)?)
  {
    return Err(evidence_error(
      "Ownership file was replaced or does not match the accepted token; automatic recovery is deferred",
    ));
  }
  let metadata = file.metadata()?;
  Ok(Some(OperationGuard {
    _file: Arc::new(file),
    directory,
    owner: owner.clone(),
    file_device: metadata.dev() as i64,
    file_inode: metadata.ino() as i64,
    _gate: gate,
  }))
}

fn unlink_exact(guard: &OperationGuard) -> IoResult<bool> {
  if Arc::strong_count(&guard._file) != 1
    || !file_matches(&guard._file, guard.file_device, guard.file_inode)?
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
  if metadata.st_dev as i64 != guard.file_device
    || metadata.st_ino as i64 != guard.file_inode
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
  if Arc::strong_count(&guard._file) != 1 {
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
  let current_boot = boot_id()?;
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
      if !try_lock(&file)? {
        return Ok(None);
      }
      let owner = read_owner(&file, key)?;
      if owner.node_name != node_name
        || (owner.owner_boot_id == current_boot
          && !file_matches(&file, owner.lock_device, owner.lock_inode)?)
      {
        return Ok(None);
      }
      let metadata = file.metadata()?;
      Ok(Some(OperationGuard {
        _file: Arc::new(file),
        directory: cursor.directory.clone(),
        owner,
        file_device: metadata.dev() as i64,
        file_inode: metadata.ino() as i64,
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
    path::PathBuf,
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
  #[ignore = "subprocess helper invoked by exclusive controller tests"]
  fn operation_gate_probe_child() {
    let state = std::env::var("NANOCL_OPERATION_LOCK_TEST_STATE").unwrap();
    let gate = acquire_daemon_guard(&state);
    println!("CONTROLLER_GATE:{}", gate.is_ok());
  }

  fn assert_other_controller_can_acquire(
    state: &StateDirectory,
    expected: bool,
  ) {
    let output = Command::new(std::env::current_exe().unwrap())
      .args([
        "--exact",
        "utils::operation::tests::operation_gate_probe_child",
        "--ignored",
        "--nocapture",
      ])
      .env("NANOCL_OPERATION_LOCK_TEST_STATE", state.path())
      .output()
      .unwrap();
    assert!(
      output.status.success(),
      "{}",
      String::from_utf8_lossy(&output.stderr)
    );
    assert!(
      String::from_utf8_lossy(&output.stdout)
        .contains(&format!("CONTROLLER_GATE:{expected}")),
      "{}",
      String::from_utf8_lossy(&output.stdout)
    );
  }

  #[test]
  fn controller_gate_is_shared_locally_and_exclusive_between_processes() {
    let state = StateDirectory::new();
    let first = acquire_daemon_guard(state.path()).unwrap();
    let alias = state.0.join(".");
    let second = acquire_daemon_guard(alias.to_str().unwrap()).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_other_controller_can_acquire(&state, false);
    drop(first);
    assert_other_controller_can_acquire(&state, false);
    drop(second);
    assert_other_controller_can_acquire(&state, true);
    assert!(state.0.join("run/operations.gate").exists());
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
    let error = abandoned_owner(&owner, "owner-test-node", state.path())
      .err()
      .unwrap();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::WouldBlock);
    assert_other_controller_can_acquire(&state, false);
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
    assert_other_controller_can_acquire(&state, false);
    finish.send(()).unwrap();
    worker.join().unwrap();
    let recovered = abandoned_owner(&owner, "owner-test-node", state.path())
      .unwrap()
      .unwrap();
    assert_other_controller_can_acquire(&state, false);
    drop(recovered);
    assert_other_controller_can_acquire(&state, true);
  }

  #[test]
  fn ambiguous_owner_evidence_never_authorizes_automatic_takeover() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    drop(guard);
    assert!(abandoned_owner(&owner, "other-node", state.path()).is_err());
    let mut wrong_boot_token = owner.clone();
    wrong_boot_token.owner_boot_id = uuid::Uuid::new_v4().to_string();
    assert!(
      abandoned_owner(&wrong_boot_token, "owner-test-node", state.path())
        .is_err()
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
  fn simulated_previous_boot_recovers_exact_token_and_pins_current_file() {
    let state = StateDirectory::new();
    let old_boot = uuid::Uuid::new_v4().to_string();
    assert_ne!(old_boot, boot_id().unwrap());
    let (owner, guard) = create_owner_with_boot_id(
      uuid::Uuid::new_v4(),
      "Vm",
      "global.previous-boot",
      uuid::Uuid::new_v4(),
      "owner-test-node",
      state.path(),
      &old_boot,
    )
    .unwrap();
    assert!(
      abandoned_owner(&owner, "owner-test-node", state.path())
        .unwrap()
        .is_none()
    );
    drop(guard);
    let path = state.lock_path(&owner);
    let original = path.with_extension("previous-boot");
    std::fs::rename(&path, &original).unwrap();
    // Simulate persistent file reappearance with changed filesystem identity.
    std::fs::copy(&original, &path).unwrap();
    let recovered = abandoned_owner(&owner, "owner-test-node", state.path())
      .unwrap()
      .unwrap();
    assert_eq!(recovered.owner, owner);
    assert_ne!(recovered.file_inode, owner.lock_inode);
    let pinned = path.with_extension("recovered");
    std::fs::rename(&path, &pinned).unwrap();
    std::fs::copy(&original, &path).unwrap();
    assert!(!unlink_exact(&recovered).unwrap());
    assert!(
      path.exists(),
      "cleanup removed a replacement after recovery"
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&pinned, &path).unwrap();
    assert!(unlink_exact(&recovered).unwrap());
  }

  #[test]
  fn same_boot_replacement_is_rejected_even_with_identical_saved_token() {
    let state = StateDirectory::new();
    let (owner, guard) = state.owner();
    drop(guard);
    let path = state.lock_path(&owner);
    let original = path.with_extension("original");
    std::fs::rename(&path, &original).unwrap();
    std::fs::copy(&original, &path).unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
    assert!(path.exists());
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
    std::fs::remove_file(&path).unwrap();
    std::fs::hard_link(path.with_extension("old"), &path).unwrap();
    assert!(abandoned_owner(&owner, "owner-test-node", state.path()).is_err());
    let run = state.0.join("run");
    let moved = state.0.join("run-original");
    std::fs::rename(&run, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &run).unwrap();
    assert!(acquire_daemon_guard(state.path()).is_err());
    assert_other_controller_can_acquire(&state, false);
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
    let (previous_boot, guard) = create_owner_with_boot_id(
      uuid::Uuid::new_v4(),
      "Vm",
      "global.previous-boot-cleanup",
      uuid::Uuid::new_v4(),
      "owner-test-node",
      state.path(),
      &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap();
    drop(guard);
    let path = state.lock_path(&previous_boot);
    let original = path.with_extension("previous-boot");
    std::fs::rename(&path, &original).unwrap();
    std::fs::copy(&original, &path).unwrap();
    expected.insert(previous_boot.key);
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
