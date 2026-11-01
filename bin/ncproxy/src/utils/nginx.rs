use std::{
  fs::{self, File, OpenOptions},
  future::Future,
  io::{Read, Seek, SeekFrom, Write},
  os::{
    fd::AsRawFd,
    unix::{
      fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
      process::CommandExt,
    },
  },
  process::Stdio,
  sync::Arc,
  time::Duration,
};

use ntex::rt;
use ntex::web;
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::UnixStream,
  process::Command,
  time::timeout,
};

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::stubs::proxy::{
  LocationTarget, ProxyHttpLocation, ProxyRule, ProxyStreamProtocol,
  ResourceProxyRule, StreamTarget,
};

use crate::models::{
  CONF_TEMPLATE, HTTP_TEMPLATE, LocationTemplate, NginxRuleKind,
  STREAM_TEMPLATE, Store, StoreConfigSnapshot, SystemStateRef,
};

use super::rule::{PreparedFile, PreparedUpstream};

const NGINX_PID_PATH: &str = "/run/nginx.pid";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_MARKER: &str = "return 200 \"ncproxy-health $pid ";
const GLOBAL_CONTROLLER_LOCK: &str = "ncproxy-controller.lock";
const GLOBAL_START_FENCE: &str = "/run/ncproxy-nginx.fence";
const GLOBAL_PENDING: &str = "ncproxy-nginx.pending";
const STATE_CONTROLLER_LOCK: &str = ".ncproxy-controller.lock";
const STATE_START_FENCE: &str = ".ncproxy-nginx.fence";
const STATE_PENDING: &str = ".ncproxy-nginx.pending";
const FENCE_READY: &[u8] = b"ncproxy-fence-v1 ready\n";

fn apply_location_options(
  template: &mut LocationTemplate,
  location: &ProxyHttpLocation,
) {
  template.enable_logging = matches!(&location.target,
    LocationTarget::Upstream(target) if target.enable_logging == Some(true));
  template.client_max_body_size =
    location.client_max_body_size.as_ref().map(|value| {
      serde_json::to_string(value)
        .unwrap()
        .trim_matches('"')
        .to_owned()
    });
  template.client_body_timeout =
    location.client_body_timeout.as_ref().map(|value| {
      serde_json::to_string(value)
        .unwrap()
        .trim_matches('"')
        .to_owned()
    });
  template.request_buffering = location.request_buffering.map(|value| {
    if value {
      "on".to_owned()
    } else {
      "off".to_owned()
    }
  });
  template.response_buffering = location.response_buffering.map(|value| {
    if value {
      "on".to_owned()
    } else {
      "off".to_owned()
    }
  });
  template.cache = location
    .cache
    .as_ref()
    .map(|value| value.as_str().to_owned());
  template.read_timeout = location.read_timeout.as_ref().map(|value| {
    serde_json::to_string(value)
      .unwrap()
      .trim_matches('"')
      .to_owned()
  });
  template.send_timeout = location.send_timeout.as_ref().map(|value| {
    serde_json::to_string(value)
      .unwrap()
      .trim_matches('"')
      .to_owned()
  });
  template.connect_timeout = location.connect_timeout.as_ref().map(|value| {
    serde_json::to_string(value)
      .unwrap()
      .trim_matches('"')
      .to_owned()
  });
  template.next_upstream =
    location.next_upstream.as_ref().map(|_| "off".to_owned());
  template.intercept_errors = location.intercept_errors.map(|value| {
    if value {
      "on".to_owned()
    } else {
      "off".to_owned()
    }
  });
}

pub async fn ensure_conf(state: &SystemStateRef) -> IoResult<()> {
  let _guard = state.config_lock.lock().await;
  let _fences = admit_fences(&state.store.dir)?;
  ensure_conf_locked(state).await
}

async fn ensure_conf_locked(state: &SystemStateRef) -> IoResult<()> {
  let state_ref = Arc::clone(state);
  let conf_path = format!("{}/nginx.conf", state_ref.store.dir);
  let default_conf = CONF_TEMPLATE.compile(&liquid::object!({
    "nginx_dir": state_ref.nginx_dir,
    "state_dir": state_ref.store.dir,
    "health_generation": new_health_generation()?,
  }))?;
  web::block(move || {
    [
      "sites-available",
      "sites-enabled",
      "streams-available",
      "streams-enabled",
      "log",
      "secrets",
    ]
    .iter()
    .map(|name| {
      fs::create_dir_all(format!("{}/{}", state_ref.store.dir, name))?;
      Ok::<_, IoError>(())
    })
    .collect::<IoResult<Vec<()>>>()?;
    Ok::<_, IoError>(())
  })
  .await?;
  log::trace!(
    "NginxManager: writing default conf to {conf_path}:\n{default_conf}"
  );
  std::fs::write(conf_path, default_conf)?;
  self::test(&state.store.dir).await?;
  Ok(())
}

fn gen_conf_path(state_dir: &str) -> String {
  format!("{state_dir}/nginx.conf")
}

// These files must never be removed or replaced while ncproxy or nginx runs.
// The directories must be controlled by the operator; advisory locks cannot
// fence older ncproxy/nginx instances or other programs that ignore them.
fn open_lock(path: &str) -> IoResult<File> {
  let file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .mode(0o600)
    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(path)?;
  let metadata = file.metadata()?;
  if !metadata.is_file()
    || metadata.nlink() != 1
    || metadata.mode() & 0o077 != 0
    || metadata.uid() != unsafe { libc::geteuid() }
  {
    return Err(IoError::other("nginx fence", "unsafe lock file"));
  }
  Ok(file)
}

fn lock_exclusive(file: &File) -> IoResult<bool> {
  match file.try_lock() {
    Ok(()) => Ok(true),
    Err(std::fs::TryLockError::WouldBlock) => Ok(false),
    Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
  }
}

pub fn controller_locks(state_dir: &str) -> IoResult<Arc<(File, File)>> {
  controller_locks_in(state_dir, "/run")
}

fn controller_locks_in(
  state_dir: &str,
  run_dir: &str,
) -> IoResult<Arc<(File, File)>> {
  fs::create_dir_all(state_dir)?;
  let dir = fs::metadata(state_dir)?;
  if !dir.is_dir()
    || dir.uid() != unsafe { libc::geteuid() }
    || dir.mode() & 0o022 != 0
  {
    return Err(IoError::other(
      "nginx fence",
      "state directory must be owned by ncproxy and not writable by other users",
    ));
  }
  let global = open_lock(&format!("{run_dir}/{GLOBAL_CONTROLLER_LOCK}"))?;
  let local = open_lock(&format!("{state_dir}/{STATE_CONTROLLER_LOCK}"))?;
  if !lock_exclusive(&global)? || !lock_exclusive(&local)? {
    return Err(IoError::other(
      "nginx fence",
      "another ncproxy controller holds the lock",
    ));
  }
  Ok(Arc::new((global, local)))
}

fn read_fence(file: &mut File) -> IoResult<Vec<u8>> {
  file.seek(SeekFrom::Start(0))?;
  let mut value = Vec::new();
  file.take(128).read_to_end(&mut value)?;
  Ok(value)
}

fn write_fence(file: &mut File, value: &[u8], parent: &str) -> IoResult<()> {
  file.set_len(0)?;
  file.seek(SeekFrom::Start(0))?;
  file.write_all(value)?;
  file.sync_all()?;
  File::open(parent)?.sync_all()?;
  Ok(())
}

fn require_no_pending(state_dir: &str) -> IoResult<()> {
  require_no_pending_in(state_dir, "/run")
}

fn require_no_pending_in(state_dir: &str, run_dir: &str) -> IoResult<()> {
  for path in [
    format!("{run_dir}/{GLOBAL_PENDING}"),
    format!("{state_dir}/{STATE_PENDING}"),
  ] {
    match fs::symlink_metadata(path) {
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
      Err(err) => return Err(err.into()),
      Ok(_) => {
        return Err(IoError::other(
          "nginx fence",
          "incomplete nginx operation requires operator intervention",
        ));
      }
    }
  }
  Ok(())
}

fn create_pending(path: &str, parent: &str) -> IoResult<()> {
  let file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
    .open(path)?;
  file.sync_all()?;
  File::open(parent)?.sync_all()?;
  Ok(())
}

fn ensure_recovery_pending(path: &str, parent: &str) -> IoResult<()> {
  match fs::symlink_metadata(path) {
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      create_pending(path, parent)
    }
    Err(err) => Err(err.into()),
    Ok(metadata)
      if metadata.is_file()
        && metadata.nlink() == 1
        && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o077 == 0 =>
    {
      let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
      let opened = file.metadata()?;
      if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
        return Err(IoError::other(
          "nginx fence",
          "pending marker changed during recovery",
        ));
      }
      file.sync_all()?;
      File::open(parent)?.sync_all()?;
      Ok(())
    }
    Ok(_) => Err(IoError::other("nginx fence", "unsafe pending marker")),
  }
}

/// Operator-authorized recovery requires all legacy processes to be stopped.
/// The acknowledgement cannot be established from PID files or locks alone.
pub fn recover_fences(state_dir: &str) -> IoResult<()> {
  let _controllers = controller_locks(state_dir)?;
  if nginx_master(&gen_conf_path(state_dir))?.is_some() {
    return Err(IoError::other(
      "nginx fence",
      "nginx master is still running",
    ));
  }
  recover_fences_in(state_dir, "/run")
}

fn recover_fences_in(state_dir: &str, run_dir: &str) -> IoResult<()> {
  let mut global = open_lock(&format!("{run_dir}/ncproxy-nginx.fence"))?;
  let mut local = open_lock(&format!("{state_dir}/{STATE_START_FENCE}"))?;
  if !lock_exclusive(&global)? || !lock_exclusive(&local)? {
    return Err(IoError::other(
      "nginx fence",
      "nginx still holds a startup lock",
    ));
  }
  ensure_recovery_pending(&format!("{run_dir}/{GLOBAL_PENDING}"), run_dir)?;
  ensure_recovery_pending(&format!("{state_dir}/{STATE_PENDING}"), state_dir)?;
  write_fence(&mut local, FENCE_READY, state_dir)?;
  write_fence(&mut global, FENCE_READY, run_dir)?;
  finish_nginx_operation_in(state_dir, run_dir)
}

fn begin_nginx_operation(state_dir: &str) -> IoResult<()> {
  begin_nginx_operation_in(state_dir, "/run")
}

fn begin_nginx_operation_in(state_dir: &str, run_dir: &str) -> IoResult<()> {
  require_no_pending_in(state_dir, run_dir)?;
  // Create and sync the global marker first, so a crash during the local
  // marker creation cannot admit a controller using another state directory.
  create_pending(&format!("{run_dir}/{GLOBAL_PENDING}"), run_dir)?;
  create_pending(&format!("{state_dir}/{STATE_PENDING}"), state_dir)
}

fn finish_nginx_operation(state_dir: &str) -> IoResult<()> {
  finish_nginx_operation_in(state_dir, "/run")
}

fn finish_nginx_operation_in(state_dir: &str, run_dir: &str) -> IoResult<()> {
  fs::remove_file(format!("{state_dir}/{STATE_PENDING}"))?;
  File::open(state_dir)?.sync_all()?;
  fs::remove_file(format!("{run_dir}/{GLOBAL_PENDING}"))?;
  File::open(run_dir)?.sync_all()?;
  Ok(())
}

fn prepare_state_fence(file: &mut File, state_dir: &str) -> IoResult<()> {
  let marker = read_fence(file)?;
  if marker == FENCE_READY {
    return Ok(());
  }
  if !marker.is_empty() {
    return Err(IoError::other(
      "nginx fence",
      "startup state is incomplete or invalid; operator intervention is required",
    ));
  }
  // Existing configuration can belong to an older child that has not yet
  // published its PID or bound its socket. Operator migration is required.
  for path in [
    gen_conf_path(state_dir),
    format!("{state_dir}/nginx-health.sock"),
  ] {
    match fs::symlink_metadata(path) {
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
      Err(err) => return Err(err.into()),
      Ok(_) => {
        return Err(IoError::other(
          "nginx fence",
          "legacy state requires operator migration",
        ));
      }
    }
  }
  write_fence(file, FENCE_READY, state_dir)
}

fn master_holds_fence(master: (u32, u64), file: &File) -> IoResult<bool> {
  let target = file.metadata()?;
  let dir = format!("/proc/{}/fd", master.0);
  for entry in fs::read_dir(dir)? {
    let entry = entry?;
    let fd = entry.file_name();
    let meta = match fs::metadata(entry.path()) {
      Ok(meta) => meta,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
      Err(err) => return Err(err.into()),
    };
    if meta.dev() != target.dev() || meta.ino() != target.ino() {
      continue;
    }
    let info = fs::read_to_string(format!(
      "/proc/{}/fdinfo/{}",
      master.0,
      fd.to_string_lossy()
    ))?;
    if info.lines().any(|line| {
      let words: Vec<_> = line.split_whitespace().collect();
      words.first() == Some(&"lock:")
        && words.get(2) == Some(&"FLOCK")
        && words.get(3) == Some(&"ADVISORY")
        && words.get(4) == Some(&"WRITE")
    }) {
      return Ok(read_process_stat(master.0)?.is_some_and(
        |(_, started, state)| {
          started == master.1 && !matches!(state, 'Z' | 'X' | 'x')
        },
      ));
    }
  }
  Ok(false)
}

// A busy fence is safe to adopt only when the current managed master has
// inherited both locks. Any partial acquisition or missing process is
// ambiguous: in particular, nginx may still be starting after a timeout.
fn admit_fences(state_dir: &str) -> IoResult<Option<(File, File)>> {
  let mut global = open_lock(GLOBAL_START_FENCE)?;
  let mut local = open_lock(&format!("{state_dir}/{STATE_START_FENCE}"))?;
  let global_locked = lock_exclusive(&global)?;
  let local_locked = lock_exclusive(&local)?;
  require_no_pending(state_dir)?;
  let master = nginx_master(&gen_conf_path(state_dir))?;
  if !global_locked || !local_locked {
    if global_locked || local_locked {
      return Err(IoError::other(
        "nginx fence",
        "startup locks have different owners",
      ));
    }
    if read_fence(&mut global)? != FENCE_READY
      || read_fence(&mut local)? != FENCE_READY
    {
      return Err(IoError::other(
        "nginx fence",
        "startup state is incomplete or invalid; operator intervention is required",
      ));
    }
    if let Some(master) = master
      && master_holds_fence(master, &global)?
      && master_holds_fence(master, &local)?
      && nginx_master(&gen_conf_path(state_dir))? == Some(master)
    {
      return Ok(None);
    }
    return Err(IoError::other(
      "nginx fence",
      "startup is in progress or ownership is ambiguous",
    ));
  }
  if master.is_some() {
    return Err(IoError::other(
      "nginx fence",
      "running nginx does not hold startup locks",
    ));
  }
  let global_marker = read_fence(&mut global)?;
  if global_marker != FENCE_READY && !global_marker.is_empty() {
    return Err(IoError::other(
      "nginx fence",
      "global startup state is incomplete or invalid; operator intervention is required",
    ));
  }
  prepare_state_fence(&mut local, state_dir)?;
  if global_marker.is_empty() {
    write_fence(&mut global, FENCE_READY, "/run")?;
  }
  Ok(Some((global, local)))
}

fn parse_pid(value: &str) -> IoResult<u32> {
  value
    .trim()
    .parse::<u32>()
    .ok()
    .filter(|pid| *pid > 1)
    .ok_or_else(|| IoError::invalid_data("nginx process", "invalid PID"))
}

/// Return parent PID, process start time and state from Linux procfs.
fn parse_process_stat(stat: &str) -> IoResult<(u32, u64, char)> {
  let fields = stat
    .rsplit_once(')')
    .map(|(_, fields)| fields.split_whitespace().collect::<Vec<_>>())
    .ok_or_else(|| IoError::invalid_data("nginx process", "invalid stat"))?;
  let parsed = (|| {
    let state = *fields.first()?;
    if state.len() != 1 {
      return None;
    }
    Some((
      fields.get(1)?.parse().ok()?,
      fields.get(19)?.parse().ok()?,
      state.chars().next()?,
    ))
  })();
  parsed.ok_or_else(|| IoError::invalid_data("nginx process", "invalid stat"))
}

fn read_process_stat(pid: u32) -> IoResult<Option<(u32, u64, char)>> {
  match fs::read_to_string(format!("/proc/{pid}/stat")) {
    Ok(stat) => parse_process_stat(&stat).map(Some),
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(err) => Err(err.into()),
  }
}

fn process_title(cmdline: &[u8]) -> &[u8] {
  cmdline.split(|byte| *byte == 0).next().unwrap_or_default()
}

fn is_master_title(title: &[u8], conf_path: &str) -> bool {
  title.starts_with(b"nginx: master process ")
    && title.ends_with(format!(" -c {conf_path}").as_bytes())
}

/// Absence is distinct from unreadable state or an unrelated PID. Only an
/// absent/dead master permits a start; a dubious PID file must fail closed.
fn nginx_master(conf_path: &str) -> IoResult<Option<(u32, u64)>> {
  let pid = match fs::read_to_string(NGINX_PID_PATH) {
    Ok(pid) => parse_pid(&pid)?,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(err) => return Err(err.into()),
  };
  let Some((_, started, state)) = read_process_stat(pid)? else {
    return Ok(None);
  };
  if matches!(state, 'Z' | 'X' | 'x') {
    return Ok(None);
  }
  let title = fs::read(format!("/proc/{pid}/cmdline"))?;
  let executable = fs::read_link(format!("/proc/{pid}/exe"))?;
  if !is_master_title(process_title(&title), conf_path)
    || !executable
      .file_name()
      .is_some_and(|name| name == "nginx" || name == "nginx (deleted)")
  {
    return Err(IoError::other(
      "nginx process",
      "PID file does not identify the managed nginx master",
    ));
  }
  if matches!(state, 'T' | 't') {
    return Err(IoError::other("nginx process", "master is stopped"));
  }
  Ok(Some((pid, started)))
}

fn child_processes(master: u32) -> IoResult<Vec<(u32, u64)>> {
  let children =
    fs::read_to_string(format!("/proc/{master}/task/{master}/children"))?;
  let mut result = Vec::new();
  for child in children.split_whitespace() {
    let pid = parse_pid(child)?;
    if let Some((parent, started, _)) = read_process_stat(pid)?
      && parent == master
    {
      // Include children whose titles still look like the master: they may
      // be old workers stuck during initialization.
      result.push((pid, started));
    }
  }
  Ok(result)
}

fn serving_worker(parent: u32, state: char, title: &[u8], master: u32) -> bool {
  parent == master
    && matches!(state, 'R' | 'S' | 'D')
    && process_title(title) == b"nginx: worker process"
}

fn new_health_generation() -> IoResult<String> {
  let mut bytes = [0u8; 16];
  openssl::rand::rand_bytes(&mut bytes).map_err(|err| {
    IoError::other("nginx health generation", &err.to_string())
  })?;
  Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn health_generation(conf: &str) -> IoResult<&str> {
  let mut matches = conf.match_indices(HEALTH_MARKER);
  let index = matches
    .next()
    .filter(|_| matches.next().is_none())
    .map(|(index, _)| index + HEALTH_MARKER.len())
    .ok_or_else(|| {
      IoError::invalid_data(
        "nginx health",
        "missing or ambiguous health marker",
      )
    })?;
  conf[index..]
    .split_once("\\n\";")
    .map(|(generation, _)| generation)
    .filter(|generation| {
      generation.len() == 32
        && generation.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
    .ok_or_else(|| {
      IoError::invalid_data("nginx health", "invalid health marker")
    })
}

fn replace_health_generation(conf: &str, generation: &str) -> IoResult<String> {
  let previous = health_generation(conf)?;
  Ok(conf.replacen(
    &format!("{HEALTH_MARKER}{previous}\\n\";"),
    &format!("{HEALTH_MARKER}{generation}\\n\";"),
    1,
  ))
}

fn parse_health_response(response: &[u8], generation: &str) -> IoResult<u32> {
  let response = std::str::from_utf8(response)
    .map_err(|_| IoError::invalid_data("nginx health", "non-UTF8 response"))?;
  let (headers, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
    IoError::invalid_data("nginx health", "incomplete HTTP response")
  })?;
  let status = headers.lines().next().unwrap_or_default();
  let mut fields = body.split_whitespace();
  let marker = fields.next();
  let pid = fields.next();
  let token = fields.next();
  if !matches!(status, "HTTP/1.0 200 OK" | "HTTP/1.1 200 OK")
    || marker != Some("ncproxy-health")
    || token != Some(generation)
    || fields.next().is_some()
    || !body.ends_with('\n')
  {
    return Err(IoError::other(
      "nginx health",
      "unexpected response or configuration generation",
    ));
  }
  parse_pid(pid.unwrap_or_default())
}

async fn probe_worker(
  state_dir: &str,
  master: (u32, u64),
  generation: &str,
) -> IoResult<(u32, u64)> {
  let pid = timeout(PROBE_TIMEOUT, async {
    let mut socket =
      UnixStream::connect(format!("{state_dir}/nginx-health.sock")).await?;
    socket
      .write_all(
        b"GET / HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n",
      )
      .await?;
    let mut response = Vec::new();
    socket.take(4097).read_to_end(&mut response).await?;
    if response.len() > 4096 {
      return Err(IoError::invalid_data("nginx health", "oversized response"));
    }
    parse_health_response(&response, generation)
  })
  .await
  .map_err(|_| IoError::other("nginx health", "local request timed out"))??;
  let stat = read_process_stat(pid)?;
  let title = fs::read(format!("/proc/{pid}/cmdline"))?;
  if let Some((parent, started, state)) = stat
    && serving_worker(parent, state, &title, master.0)
    && nginx_master(&gen_conf_path(state_dir))? == Some(master)
  {
    return Ok((pid, started));
  }
  Err(IoError::other(
    "nginx health",
    "response was not from a live worker of the current master",
  ))
}

async fn wait_for_worker(
  state_dir: &str,
  master: (u32, u64),
  generation: &str,
  previous: &[(u32, u64)],
) -> IoResult<()> {
  timeout(READY_TIMEOUT, async {
    loop {
      if nginx_master(&gen_conf_path(state_dir))? != Some(master) {
        return Err(IoError::other(
          "nginx health",
          "master changed while waiting",
        ));
      }
      if let Ok(worker) = probe_worker(state_dir, master, generation).await
        && !previous.contains(&worker)
      {
        return Ok(());
      }
      ntex::time::sleep(Duration::from_millis(100)).await;
    }
  })
  .await
  .map_err(|_| {
    IoError::other(
      "nginx health",
      "new serving worker did not become ready within 10s",
    )
  })?
}

fn nginx_command(args: &[&str], conf_path: &str) -> Command {
  // nginx -t opens/creates its PID file without writing a PID. Route that
  // check to /dev/null so validation cannot leave an empty runtime PID file,
  // including when the check is cancelled or fails.
  let pid_path = if args.contains(&"-t") {
    "/dev/null"
  } else {
    NGINX_PID_PATH
  };
  let mut command = Command::new("nginx");
  command
    .args(args)
    .arg("-g")
    .arg(format!("pid {pid_path};"))
    .arg("-c")
    .arg(conf_path)
    .stdin(Stdio::null())
    .kill_on_drop(true);
  command
}

async fn exec_nginx_cmd(args: &[&str], conf_path: &str) -> IoResult<()> {
  let output = exec_nginx_output(args, conf_path).await?;
  check_nginx_output(output)
}

async fn exec_nginx_output(
  args: &[&str],
  conf_path: &str,
) -> IoResult<std::process::Output> {
  Ok(
    timeout(COMMAND_TIMEOUT, nginx_command(args, conf_path).output())
      .await
      .map_err(|_| IoError::other("exec", "nginx command timed out"))??,
  )
}

fn check_nginx_output(output: std::process::Output) -> IoResult<()> {
  if output.status.success() {
    return Ok(());
  }
  let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
  message.push_str(String::from_utf8_lossy(&output.stderr).as_ref());
  Err(IoError::other("exec", &message))
}

/// Do not capture streams inherited by the daemon: EOF may never arrive.
async fn start_nginx_cmd(
  conf_path: &str,
  fences: &(File, File),
) -> IoResult<()> {
  let mut command = nginx_command(&[], conf_path);
  let fds = [fences.0.as_raw_fd(), fences.1.as_raw_fd()];
  // The parent retains close-on-exec. Only the child clears it, so nginx's
  // master inherits the locked open-file descriptions through fork/exec.
  unsafe {
    command.as_std_mut().pre_exec(move || {
      for fd in fds {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0
          || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0
        {
          return Err(std::io::Error::last_os_error());
        }
      }
      Ok(())
    });
  }
  let status = timeout(
    COMMAND_TIMEOUT,
    command.stdout(Stdio::null()).stderr(Stdio::null()).status(),
  )
  .await
  .map_err(|_| IoError::other("exec", "nginx start timed out"))??;
  if status.success() {
    return Ok(());
  }
  Err(IoError::other(
    "exec",
    &format!("nginx failed to start with status {status}"),
  ))
}

fn confirm_health_socket_absent(path: &str) -> IoResult<()> {
  match fs::symlink_metadata(path) {
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(err) => Err(err.into()),
    Ok(_) => Err(IoError::other(
      "nginx health socket",
      "health socket path appeared during cleanup",
    )),
  }
}

/// Remove only a disconnected health socket, preserving live or unrelated paths.
async fn remove_stale_health_socket(state_dir: &str) -> IoResult<()> {
  let path = format!("{state_dir}/nginx-health.sock");
  let metadata = match fs::symlink_metadata(&path) {
    Ok(metadata) => metadata,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(err) => return Err(err.into()),
  };
  if !metadata.file_type().is_socket() {
    return Err(IoError::other(
      "nginx health socket",
      "health socket path is not a socket",
    ));
  }
  match timeout(PROBE_TIMEOUT, UnixStream::connect(&path)).await {
    Ok(Ok(_)) => {
      return Err(IoError::other(
        "nginx health socket",
        "health socket is still accepting connections",
      ));
    }
    Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => {
      return confirm_health_socket_absent(&path);
    }
    Ok(Err(err)) if err.kind() == std::io::ErrorKind::ConnectionRefused => (),
    Ok(Err(err)) => return Err(err.into()),
    Err(_) => {
      return Err(IoError::other(
        "nginx health socket",
        "health socket connection timed out",
      ));
    }
  }
  // Do not delete a replacement installed while the liveness probe ran.
  let current = match fs::symlink_metadata(&path) {
    Ok(metadata) => metadata,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(err) => return Err(err.into()),
  };
  if !current.file_type().is_socket()
    || current.dev() != metadata.dev()
    || current.ino() != metadata.ino()
  {
    return Err(IoError::other(
      "nginx health socket",
      "health socket path changed during cleanup",
    ));
  }
  match fs::remove_file(&path) {
    Ok(()) => Ok(()),
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(err) => Err(err.into()),
  }
}

pub async fn ensure_started(state_dir: &str) -> IoResult<()> {
  let Some(fences) = admit_fences(state_dir)? else {
    return Ok(());
  };
  start_with_fences(state_dir, fences).await
}

pub async fn initialize(state: &SystemStateRef) -> IoResult<()> {
  let _guard = state.config_lock.lock().await;
  let fences = admit_fences(&state.store.dir)?;
  ensure_conf_locked(state).await?;
  if let Some(fences) = fences {
    start_with_fences(&state.store.dir, fences).await?;
  }
  Ok(())
}

async fn start_with_fences(
  state_dir: &str,
  fences: (File, File),
) -> IoResult<()> {
  let conf_path = gen_conf_path(state_dir);
  match fs::remove_file(NGINX_PID_PATH) {
    Ok(()) => (),
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
    Err(err) => return Err(err.into()),
  }
  remove_stale_health_socket(state_dir).await?;
  log::warn!("nginx: master is absent or dead, starting");
  // A crash or cancellation at any point after this leaves a persistent
  // ambiguous state. Never infer safety from a free lock in that case.
  begin_nginx_operation(state_dir)?;
  start_nginx_cmd(&conf_path, &fences).await?;
  let conf = fs::read_to_string(&conf_path)?;
  let generation = health_generation(&conf)?;
  timeout(READY_TIMEOUT, async {
    // The daemonizing parent can exit before its child publishes the PID file
    // and updates its process title. Only this post-launch wait retries both.
    let master = loop {
      if let Ok(Some(master)) = nginx_master(&conf_path) {
        break master;
      }
      ntex::time::sleep(Duration::from_millis(100)).await;
    };
    if !master_holds_fence(master, &fences.0)?
      || !master_holds_fence(master, &fences.1)?
      || nginx_master(&conf_path)? != Some(master)
    {
      return Err(IoError::other(
        "nginx fence",
        "master did not inherit startup locks",
      ));
    }
    wait_for_worker(state_dir, master, generation, &[]).await?;
    if nginx_master(&conf_path)? != Some(master)
      || !master_holds_fence(master, &fences.0)?
      || !master_holds_fence(master, &fences.1)?
    {
      return Err(IoError::other(
        "nginx fence",
        "master changed or lost startup locks before readiness",
      ));
    }
    Ok::<_, IoError>(())
  })
  .await
  .map_err(|_| {
    IoError::other(
      "nginx start",
      "master and worker did not become ready within 10s",
    )
  })??;
  finish_nginx_operation(state_dir)?;
  Ok(())
}

pub fn spawn(state: &SystemStateRef) {
  let state_dir = state.store.dir.clone();
  let config_lock = Arc::clone(&state.config_lock);
  rt::Arbiter::new().handle().spawn(async move {
    loop {
      {
        let _guard = config_lock.lock().await;
        let health = async {
          ensure_started(&state_dir).await?;
          let conf_path = gen_conf_path(&state_dir);
          let master = nginx_master(&conf_path)?.ok_or_else(|| {
            IoError::other("nginx health", "master disappeared")
          })?;
          let conf = fs::read_to_string(&conf_path)?;
          probe_worker(&state_dir, master, health_generation(&conf)?).await
        }
        .await;
        if let Err(err) = health {
          // Repeated HUPs can accumulate workers stuck during initialization.
          // Keep a live unhealthy master intact and report the failed probe.
          log::warn!("nginx::monitor: {err}");
        }
      }
      ntex::time::sleep(Duration::from_secs(2)).await;
    }
  });
}

pub async fn test(state_dir: &str) -> IoResult<()> {
  log::info!("nginx::test: starting");
  begin_nginx_operation(state_dir)?;
  let output = exec_nginx_output(&["-t"], &gen_conf_path(state_dir)).await?;
  // A normal nonzero exit was reaped, so rollback is safe. Timeouts,
  // cancellation and I/O errors leave the pending markers in place.
  finish_nginx_operation(state_dir)?;
  check_nginx_output(output)?;
  log::info!("nginx::test: done");
  Ok(())
}

pub async fn reload(state_dir: &str) -> IoResult<()> {
  log::info!("nginx::reload: starting");
  ensure_started(state_dir).await?;
  let conf_path = gen_conf_path(state_dir);
  let master = nginx_master(&conf_path)?.ok_or_else(|| {
    IoError::other("nginx reload", "master disappeared before reload")
  })?;
  let previous = child_processes(master.0)?;
  let conf = fs::read_to_string(&conf_path)?;
  let generation = new_health_generation()?;
  fs::write(&conf_path, replace_health_generation(&conf, &generation)?)?;
  self::test(state_dir).await?;
  if nginx_master(&conf_path)? != Some(master) {
    return Err(IoError::other(
      "nginx reload",
      "master changed before reload",
    ));
  }
  begin_nginx_operation(state_dir)?;
  exec_nginx_cmd(&["-s", "reload"], &conf_path).await?;
  wait_for_worker(state_dir, master, &generation, &previous).await?;
  finish_nginx_operation(state_dir)?;
  log::info!("nginx::reload: done");
  Ok(())
}

async fn rollback_config(
  snapshot: &StoreConfigSnapshot,
  original_error: IoError,
  store: &Store,
) -> IoResult<()> {
  match store.restore_config(snapshot).await {
    Ok(()) => Err(original_error),
    Err(rollback_error) => Err(IoError::other(
      "nginx configuration rollback",
      &format!(
        "candidate failed: {original_error}; rollback failed: {rollback_error}"
      ),
    )),
  }
}

async fn config_transaction<F, Fut>(
  store: &Store,
  credential_paths: &[&str],
  operation: F,
) -> IoResult<()>
where
  F: FnOnce() -> Fut,
  Fut: Future<Output = IoResult<()>>,
{
  let snapshot = store.snapshot_config(credential_paths).await?;
  match operation().await {
    Ok(()) => Ok(()),
    Err(err) => {
      if require_no_pending(&store.dir).is_err() {
        return Err(IoError::other(
          "nginx configuration",
          &format!(
            "{err}; rollback skipped because nginx operation is incomplete"
          ),
        ));
      }
      rollback_config(&snapshot, err, store).await
    }
  }
}

pub async fn add_rule(
  name: &str,
  rule: &ResourceProxyRule,
  state: &SystemStateRef,
) -> IoResult<()> {
  let prepared = prepare_rule(name, rule, state).await?;
  let _guard = state.config_lock.lock().await;
  let _fences = admit_fences(&state.store.dir)?;
  let credential_paths = prepared
    .files
    .iter()
    .map(|file| file.path.as_str())
    .collect::<Vec<_>>();
  config_transaction(&state.store, &credential_paths, || async {
    apply_prepared_rule(name, &prepared, state).await?;
    self::test(&state.store.dir).await
  })
  .await
}

pub(crate) struct PreparedRule {
  site: Option<String>,
  stream: Option<String>,
  upstreams: Vec<(PreparedUpstream, NginxRuleKind)>,
  files: Vec<PreparedFile>,
}

async fn prepare_rule(
  name: &str,
  rule: &ResourceProxyRule,
  state: &SystemStateRef,
) -> IoResult<PreparedRule> {
  // Validate before resolving upstreams or preparing any credential files.
  if rule.rules.iter().any(|rule| {
    matches!(rule, ProxyRule::Stream(stream)
      if matches!(stream.protocol, ProxyStreamProtocol::Udp) && stream.ssl.is_some())
  }) {
    return Err(IoError::invalid_input(
      "Proxy stream",
      "TLS is not supported for UDP stream rules",
    ));
  }
  let mut stream_conf = String::new();
  let mut http_conf = String::new();
  let mut upstreams = Vec::new();
  let mut files = Vec::new();
  for rule in &rule.rules {
    match rule {
      ProxyRule::Stream(stream_rule) => {
        let listen = super::rule::get_network_addr(
          &stream_rule.network,
          stream_rule.port,
          &state.client,
        )
        .await?;
        let upstream =
          match super::rule::gen_stream_upstream(&stream_rule.target, state)
            .await
          {
            Err(err) => {
              log::warn!("{err} {:#?}", stream_rule.target);
              continue;
            }
            Ok(upstream) => upstream,
          };
        let ssl = match &stream_rule.ssl {
          Some(ssl) => match super::rule::gen_ssl_config(ssl, state).await {
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
            Ok(prepared) => {
              files.extend(prepared.files);
              Some(prepared.config)
            }
          },
          None => None,
        };
        if stream_rule.ssl.is_some() && ssl.is_none() {
          log::warn!("Not ssl found for {name} {ssl:#?}");
          continue;
        }
        let data = STREAM_TEMPLATE.compile(&liquid::object!({
          "state_dir": state.store.dir,
          "enable_logging": matches!(&stream_rule.target,
            StreamTarget::Upstream(target) if target.enable_logging == Some(true)),
          "listen": listen,
          "udp": matches!(stream_rule.protocol, ProxyStreamProtocol::Udp),
          "key": name,
          "upstream_key": upstream.key,
          "ssl": ssl,
        }))?;
        upstreams.push((upstream, NginxRuleKind::Stream));
        stream_conf += &data;
      }
      ProxyRule::Http(http_rule) => {
        let mut locations = vec![];
        let listen = super::rule::get_network_addr(
          &http_rule.network,
          http_rule.port.unwrap_or(80),
          &state.client,
        )
        .await?;
        let listen_https = super::rule::get_network_addr(
          &http_rule.network,
          http_rule.port.unwrap_or(443),
          &state.client,
        )
        .await?;
        let ssl = match &http_rule.ssl {
          Some(ssl) => match super::rule::gen_ssl_config(ssl, state).await {
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
            Ok(prepared) => {
              files.extend(prepared.files);
              Some(prepared.config)
            }
          },
          None => None,
        };
        let hsts_config =
          super::rule::gen_hsts_config(http_rule.hsts.clone()).await;
        for location in &http_rule.locations {
          match &location.target {
            LocationTarget::Upstream(upstream) => {
              let prepared_upstream =
                match super::rule::gen_upstream(upstream, state).await {
                  Err(err) => {
                    log::warn!("{err} {:#?}", upstream);
                    continue;
                  }
                  Ok(upstream) => upstream,
                };
              let ssl = match &upstream.ssl {
                Some(ssl) => {
                  match super::rule::gen_ssl_config(ssl, state).await {
                    Err(err) => {
                      log::warn!("Not ssl found for {name} {ssl:#?} {err}");
                      None
                    }
                    Ok(prepared) => {
                      files.extend(prepared.files);
                      Some(prepared.config)
                    }
                  }
                }
                None => None,
              };
              let mut rendered = LocationTemplate {
                path: location.path.clone(),
                enable_logging: false,
                limit_req: location.limit_req.clone(),
                upstream_key: if ssl.is_some() {
                  format!("https://{}", prepared_upstream.key)
                } else {
                  format!("http://{}", prepared_upstream.key)
                },
                redirect: None,
                upstream_path: upstream.path.clone().unwrap_or("/".to_owned()),
                version: location.version,
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                ssl,
                client_max_body_size: None,
                client_body_timeout: None,
                request_buffering: None,
                response_buffering: None,
                cache: None,
                read_timeout: None,
                send_timeout: None,
                connect_timeout: None,
                next_upstream: None,
                intercept_errors: None,
              };
              apply_location_options(&mut rendered, location);
              upstreams.push((prepared_upstream, NginxRuleKind::Site));
              locations.push(rendered);
            }
            LocationTarget::Unix(unix) => {
              let prepared_upstream =
                super::rule::gen_unix_target(unix).await?;
              let mut rendered = LocationTemplate {
                path: location.path.clone(),
                enable_logging: false,
                upstream_key: format!("http://{}", prepared_upstream.key),
                redirect: None,
                limit_req: location.limit_req.clone(),
                upstream_path: "/".to_owned(),
                version: location.version,
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                ssl: None,
                client_max_body_size: None,
                client_body_timeout: None,
                request_buffering: None,
                response_buffering: None,
                cache: None,
                read_timeout: None,
                send_timeout: None,
                connect_timeout: None,
                next_upstream: None,
                intercept_errors: None,
              };
              apply_location_options(&mut rendered, location);
              upstreams.push((prepared_upstream, NginxRuleKind::Site));
              locations.push(rendered);
            }
            LocationTarget::Http(http) => {
              let mut rendered = LocationTemplate {
                path: location.path.clone(),
                enable_logging: false,
                upstream_key: http.url.clone(),
                limit_req: location.limit_req.clone(),
                version: location.version,
                upstream_path: "/".to_owned(),
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                redirect: http.redirect.clone().map(|r| format!("{r}")),
                ssl: None,
                client_max_body_size: None,
                client_body_timeout: None,
                request_buffering: None,
                response_buffering: None,
                cache: None,
                read_timeout: None,
                send_timeout: None,
                connect_timeout: None,
                next_upstream: None,
                intercept_errors: None,
              };
              apply_location_options(&mut rendered, location);
              locations.push(rendered);
            }
          }
        }
        let data = HTTP_TEMPLATE.compile(&liquid::object!({
          "state_dir": state.store.dir,
          "key": name,
          "limit_req_zone": http_rule.limit_req_zone,
          "listen": listen,
          "listen_https": listen_https,
          "domain": http_rule.domain,
          "locations": locations,
          "ssl": ssl,
          "hsts": hsts_config,
          "http3": http_rule.http3,
          "hide_upstream": http_rule.ssl.is_some() && ssl.is_none(),
        }))?;
        http_conf += &data;
      }
    }
  }
  Ok(PreparedRule {
    site: (!http_conf.is_empty()).then_some(http_conf),
    stream: (!stream_conf.is_empty()).then_some(stream_conf),
    upstreams,
    files,
  })
}

async fn apply_prepared_rule(
  name: &str,
  prepared: &PreparedRule,
  state: &SystemStateRef,
) -> IoResult<()> {
  for file in &prepared.files {
    tokio::fs::write(&file.path, &file.data).await?;
  }
  for (upstream, kind) in &prepared.upstreams {
    state
      .store
      .write_conf_file(&upstream.key, &upstream.content, kind)
      .await?;
  }
  state
    .store
    .replace_rule(name, prepared.site.as_deref(), prepared.stream.as_deref())
    .await
}

/// Replace every managed rule from a persisted nanocld snapshot.
///
/// Clearing first removes resources deleted while ncproxy was offline. The
/// previous complete directory state is restored when rendering or nginx
/// validation rejects any candidate rule.
pub(crate) async fn rebuild_rules(
  rules: &[(String, ResourceProxyRule)],
  state: &SystemStateRef,
) -> IoResult<()> {
  let mut prepared_rules = Vec::with_capacity(rules.len());
  for (name, rule) in rules {
    prepared_rules.push((name, prepare_rule(name, rule, state).await?));
  }
  let _guard = state.config_lock.lock().await;
  let _fences = admit_fences(&state.store.dir)?;
  let credential_paths = prepared_rules
    .iter()
    .flat_map(|(_, prepared)| {
      prepared.files.iter().map(|file| file.path.as_str())
    })
    .collect::<Vec<_>>();
  config_transaction(&state.store, &credential_paths, || async {
    state.store.clear_config().await?;
    for (name, prepared) in &prepared_rules {
      apply_prepared_rule(name, prepared, state).await?;
    }
    self::test(&state.store.dir).await
  })
  .await
}

pub async fn del_rule(name: &str, state: &SystemStateRef) -> IoResult<()> {
  let _guard = state.config_lock.lock().await;
  let _fences = admit_fences(&state.store.dir)?;
  config_transaction(&state.store, &[], || async {
    state.store.replace_rule(name, None, None).await
  })
  .await
}

#[cfg(test)]
mod tests {
  use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
  };

  use super::*;

  const GENERATION: &str = "0123456789abcdef0123456789abcdef";
  const NEXT_GENERATION: &str = "abcdef0123456789abcdef0123456789";

  fn test_state(dir: &str) -> SystemStateRef {
    let (sender, _) = futures::channel::mpsc::unbounded();
    Arc::new(crate::models::SystemState {
      store: Store::new(dir),
      client: nanocld_client::NanocldClient::connect_with_unix_default(),
      event_emitter: crate::models::EventEmitter(Arc::new(sender)),
      nginx_dir: String::new(),
      config_lock: Arc::new(futures::lock::Mutex::new(())),
      _controller_locks: None,
    })
  }

  #[ntex::test]
  async fn stream_preparation_honors_tcp_and_udp_protocols() {
    let state = test_state("unused");
    for (protocol, tls, listener) in [
      ("Tcp", false, "listen 127.0.0.1:9000;"),
      ("Udp", false, "listen 127.0.0.1:9000 udp;"),
      ("Tcp", true, "listen 127.0.0.1:9000 ssl;"),
    ] {
      let ssl = tls.then(|| {
        serde_json::json!({
          "Certificate": "/unused/server.crt",
          "CertificateKey": "/unused/server.key",
        })
      });
      let rule: ResourceProxyRule = serde_json::from_value(serde_json::json!({
        "Rules": [{
          "Network": "Local",
          "Protocol": protocol,
          "Ssl": ssl,
          "Port": 9000,
          "Target": {"UnixPath": "/unused/upstream.sock"},
        }],
      }))
      .unwrap();
      let prepared = prepare_rule("stream", &rule, &state).await.unwrap();
      let stream = prepared.stream.unwrap();
      let listeners = stream
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| line.starts_with("listen "))
        .collect::<Vec<_>>();
      assert_eq!(listeners, vec![listener]);
      assert_eq!(stream.contains("ssl_certificate "), tls);
      assert!(prepared.files.is_empty());
      assert!(prepared.site.is_none());
      assert_eq!(prepared.upstreams.len(), 1);
    }
  }

  #[ntex::test]
  async fn stream_preparation_rejects_udp_tls() {
    let state = test_state("unused");
    let rule: ResourceProxyRule = serde_json::from_value(serde_json::json!({
      "Rules": [{
        "Network": "Local",
        "Protocol": "Udp",
        "Port": 9000,
        "Target": {"UnixPath": "/unused/upstream.sock"},
        "Ssl": {
          "Certificate": "/unused/server.crt",
          "CertificateKey": "/unused/server.key",
        },
      }],
    }))
    .unwrap();
    let error = prepare_rule("stream", &rule, &state)
      .await
      .err()
      .expect("UDP with TLS must be rejected before preparing a candidate");
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("TLS is not supported for UDP"));
  }

  #[ntex::test]
  async fn rejected_transaction_restores_all_credentials_and_removes_new_files()
  {
    let test_dir = TestDir::new();
    let state = test_state(test_dir.path().to_str().unwrap());
    std::fs::create_dir(test_dir.path().join("secrets")).unwrap();
    let mut files = Vec::new();
    for suffix in ["cert", "key", "ca", "pem"] {
      let path = test_dir.path().join(format!("secrets/accepted.{suffix}"));
      // Snapshots preserve bytes, including content that is not UTF-8.
      std::fs::write(&path, [0xff, 0, 42]).unwrap();
      files.push(PreparedFile {
        path: path.to_str().unwrap().to_owned(),
        data: "invalid candidate credentials".to_owned(),
      });
      files.push(PreparedFile {
        path: test_dir
          .path()
          .join(format!("secrets/new.{suffix}"))
          .to_str()
          .unwrap()
          .to_owned(),
        data: "new candidate credentials".to_owned(),
      });
    }
    state
      .store
      .write_conf_file("rule", "accepted", &NginxRuleKind::Site)
      .await
      .unwrap();
    let prepared = PreparedRule {
      site: Some("rejected".to_owned()),
      stream: None,
      upstreams: Vec::new(),
      files,
    };
    let paths = prepared
      .files
      .iter()
      .map(|file| file.path.as_str())
      .collect::<Vec<_>>();
    let before = state.store.snapshot_config(&paths).await.unwrap();
    let result = config_transaction(&state.store, &paths, || async {
      apply_prepared_rule("rule", &prepared, &state).await?;
      Err(IoError::invalid_data(
        "candidate",
        "TLS validation rejected",
      ))
    })
    .await;
    assert!(
      result
        .unwrap_err()
        .to_string()
        .contains("TLS validation rejected")
    );
    assert_eq!(state.store.snapshot_config(&paths).await.unwrap(), before);
  }

  #[ntex::test]
  async fn partial_credential_write_restores_earlier_writes() {
    let test_dir = TestDir::new();
    let state = test_state(test_dir.path().to_str().unwrap());
    let old_path = test_dir.path().join("accepted.cert");
    let new_path = test_dir.path().join("new.ca");
    let failed_path = test_dir.path().join("missing-parent/new.key");
    std::fs::write(&old_path, "accepted certificate").unwrap();
    let prepared = PreparedRule {
      site: None,
      stream: None,
      upstreams: Vec::new(),
      files: [old_path, new_path, failed_path]
        .into_iter()
        .map(|path| PreparedFile {
          path: path.to_str().unwrap().to_owned(),
          data: "candidate".to_owned(),
        })
        .collect(),
    };
    let paths = prepared
      .files
      .iter()
      .map(|file| file.path.as_str())
      .collect::<Vec<_>>();
    let before = state.store.snapshot_config(&paths).await.unwrap();
    let result = config_transaction(&state.store, &paths, || async {
      apply_prepared_rule("rule", &prepared, &state).await
    })
    .await;
    assert!(result.is_err());
    assert_eq!(state.store.snapshot_config(&paths).await.unwrap(), before);
  }

  #[ntex::test]
  async fn credential_rollback_continues_after_one_restore_failure() {
    let test_dir = TestDir::new();
    let store = Store::new(test_dir.path().to_str().unwrap());
    let blocked = test_dir.path().join("a-blocked");
    let old = test_dir.path().join("b-existing.cert");
    let new = test_dir.path().join("c-new.key");
    std::fs::write(&old, "accepted").unwrap();
    store
      .write_conf_file("rule", "accepted", &NginxRuleKind::Site)
      .await
      .unwrap();
    let paths = [
      blocked.to_str().unwrap(),
      old.to_str().unwrap(),
      new.to_str().unwrap(),
    ];
    let result = config_transaction(&store, &paths, || async {
      // An inaccessible path must not prevent rollback of other files.
      std::fs::create_dir(&blocked)?;
      std::fs::write(&old, "candidate")?;
      std::fs::write(&new, "candidate")?;
      store
        .write_conf_file("rule", "candidate", &NginxRuleKind::Site)
        .await?;
      Err(IoError::invalid_data("candidate", "rejected"))
    })
    .await;
    assert!(result.unwrap_err().to_string().contains("rollback failed"));
    assert_eq!(std::fs::read_to_string(&old).unwrap(), "accepted");
    assert!(!new.exists());
    assert_eq!(
      store.snapshot_config(&[]).await.unwrap().sites["rule.conf"],
      "accepted"
    );
  }

  #[test]
  fn http_access_logging_requires_target_opt_in() {
    let cases = [
      (
        serde_json::json!({"Key": "app.global.c", "Port": 80}),
        false,
      ),
      (
        serde_json::json!({"Key": "app.global.c", "Port": 80, "EnableLogging": false}),
        false,
      ),
      (
        serde_json::json!({"Key": "app.global.c", "Port": 80, "EnableLogging": true}),
        true,
      ),
      (serde_json::json!({"Url": "http://example.test"}), false),
      (serde_json::json!({"UnixPath": "/run/app.sock"}), false),
    ];
    let locations = cases
      .iter()
      .enumerate()
      .map(|(index, (target, _))| {
        let location: ProxyHttpLocation =
          serde_json::from_value(serde_json::json!({
            "Path": format!("/route-{index}/"),
            "Target": target,
          }))
          .unwrap();
        let mut rendered: LocationTemplate =
          serde_json::from_value(serde_json::json!({
            "path": location.path,
            "enable_logging": false,
            "upstream_key": "http://upstream",
            "upstream_path": "/",
          }))
          .unwrap();
        apply_location_options(&mut rendered, &location);
        rendered
      })
      .collect::<Vec<_>>();
    let output = HTTP_TEMPLATE
      .compile(&liquid::object!({
        "key": "logging-test",
        "listen": "127.0.0.1:80",
        "state_dir": "/tmp/ncproxy",
        "locations": locations,
        "hide_upstream": false,
      }))
      .unwrap();
    for (index, (_, enabled)) in cases.iter().enumerate() {
      let marker = format!("location /route-{index}/ {{");
      let block = output
        .split_once(&marker)
        .unwrap()
        .1
        .split_once('}')
        .unwrap()
        .0;
      let directive = if *enabled {
        "access_log /tmp/ncproxy/log/access.log http_basic;"
      } else {
        "access_log off;"
      };
      assert!(block.contains(directive), "{block}");
      assert_eq!(block.matches("access_log ").count(), 1);
    }
  }

  #[test]
  fn supervision_rejects_invalid_pids_and_unrelated_master_titles() {
    for invalid in ["", "-1", "0", "1", "not-a-pid", "4294967296"] {
      assert!(parse_pid(invalid).is_err(), "{invalid}");
    }
    assert_eq!(parse_pid(" 42\n").unwrap(), 42);
    let conf = "/var/lib/ncproxy/nginx.conf";
    assert!(is_master_title(
      b"nginx: master process nginx -c /var/lib/ncproxy/nginx.conf",
      conf,
    ));
    for title in [
      &b"nginx: worker process"[..],
      &b"unrelated -c /var/lib/ncproxy/nginx.conf"[..],
      &b"nginx: master process nginx -c /other/nginx.conf"[..],
      &b"nginx: master process nginx -c /var/lib/ncproxy/nginx.conf.bak"[..],
    ] {
      assert!(!is_master_title(title, conf));
    }
  }

  #[test]
  fn supervision_parses_proc_identity_without_confusing_comm_parentheses() {
    let stat = format!("42 (worker (init)) S 7 {} 12345", "0 ".repeat(17));
    assert_eq!(parse_process_stat(&stat).unwrap(), (7, 12345, 'S'));
    assert!(parse_process_stat("42 (nginx) S 7").is_err());
    assert!(parse_process_stat("42 nginx S 7").is_err());
  }

  #[test]
  fn supervision_rejects_initializing_exiting_and_dead_workers() {
    assert!(serving_worker(7, 'S', b"nginx: worker process\0\0", 7));
    assert!(!serving_worker(8, 'S', b"nginx: worker process", 7));
    for state in ['Z', 'X', 'x', 'T', 't', '?'] {
      assert!(!serving_worker(7, state, b"nginx: worker process", 7));
    }
    for title in [
      &b"nginx: worker process is shutting down"[..],
      &b"nginx: master process nginx -c /nginx.conf"[..],
      &b"nginx: cache manager process"[..],
    ] {
      assert!(!serving_worker(7, 'S', title, 7));
    }
  }

  #[test]
  fn supervision_changes_only_the_unique_health_generation() {
    let conf = format!(
      "# keep {GENERATION}\n  {HEALTH_MARKER}{GENERATION}\\n\";\n# unchanged\n"
    );
    let changed = replace_health_generation(&conf, NEXT_GENERATION).unwrap();
    assert_eq!(health_generation(&changed).unwrap(), NEXT_GENERATION);
    assert!(changed.starts_with(&format!("# keep {GENERATION}\n")));
    assert!(changed.ends_with("# unchanged\n"));
    assert!(replace_health_generation("no marker", NEXT_GENERATION).is_err());
    assert!(
      replace_health_generation(&conf.repeat(2), NEXT_GENERATION).is_err()
    );
    assert!(health_generation(&format!("{HEALTH_MARKER}bad\\n\";")).is_err());
  }

  #[test]
  fn supervision_requires_a_complete_response_from_the_requested_generation() {
    let response = format!(
      "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nncproxy-health 42 {GENERATION}\n"
    );
    assert_eq!(
      parse_health_response(response.as_bytes(), GENERATION).unwrap(),
      42
    );
    // A newly respawned PID running old configuration cannot prove reload.
    assert!(
      parse_health_response(response.as_bytes(), NEXT_GENERATION).is_err()
    );
    assert!(
      parse_health_response(response.trim_end().as_bytes(), GENERATION)
        .is_err()
    );
    assert!(
      parse_health_response(
        response.replace("200 OK", "503 Unavailable").as_bytes(),
        GENERATION,
      )
      .is_err()
    );
    assert!(
      parse_health_response(
        response
          .replace("ncproxy-health 42", "ncproxy-health -1")
          .as_bytes(),
        GENERATION,
      )
      .is_err()
    );
    assert!(
      parse_health_response(
        response
          .replace("ncproxy-health", "some-other-server")
          .as_bytes(),
        GENERATION,
      )
      .is_err()
    );
  }

  static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

  struct TestDir(PathBuf);

  impl TestDir {
    fn new() -> Self {
      let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
      let path = std::env::temp_dir()
        .join(format!("ncproxy-config-test-{}-{id}", std::process::id()));
      for dir in [
        "sites-available",
        "sites-enabled",
        "streams-available",
        "streams-enabled",
      ] {
        std::fs::create_dir_all(path.join(dir)).unwrap();
      }
      Self(path)
    }

    fn path(&self) -> &Path {
      &self.0
    }
  }

  impl Drop for TestDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  #[test]
  fn fence_lock_remains_held_by_a_duplicate_descriptor() {
    let dir = TestDir::new();
    let path = dir.path().join("fence");
    let path = path.to_str().unwrap();
    let locked = open_lock(path).unwrap();
    assert!(lock_exclusive(&locked).unwrap());
    let duplicate = locked.try_clone().unwrap();
    let other = open_lock(path).unwrap();
    assert!(!lock_exclusive(&other).unwrap());
    let pid = std::process::id();
    let (_, started, _) = read_process_stat(pid).unwrap().unwrap();
    assert!(master_holds_fence((pid, started), &other).unwrap());
    drop(locked);
    assert!(!lock_exclusive(&other).unwrap());
    drop(duplicate);
    assert!(lock_exclusive(&other).unwrap());
  }

  #[test]
  fn state_fence_rejects_legacy_and_incomplete_startup() {
    let dir = TestDir::new();
    let state_dir = dir.path().to_str().unwrap();
    let path = dir.path().join(STATE_START_FENCE);
    let mut fence = open_lock(path.to_str().unwrap()).unwrap();
    assert!(lock_exclusive(&fence).unwrap());
    std::fs::write(dir.path().join("nginx.conf"), "legacy").unwrap();
    assert!(prepare_state_fence(&mut fence, state_dir).is_err());
    assert!(read_fence(&mut fence).unwrap().is_empty());
    assert!(prepare_state_fence(&mut fence, state_dir).is_err());
    std::fs::remove_file(dir.path().join("nginx.conf")).unwrap();
    prepare_state_fence(&mut fence, state_dir).unwrap();
    assert_eq!(read_fence(&mut fence).unwrap(), FENCE_READY);
    write_fence(&mut fence, b"invalid", state_dir).unwrap();
    assert!(prepare_state_fence(&mut fence, state_dir).is_err());
    assert_eq!(read_fence(&mut fence).unwrap(), b"invalid");
    let pending = dir.path().join(STATE_PENDING);
    create_pending(pending.to_str().unwrap(), state_dir).unwrap();
    assert!(create_pending(pending.to_str().unwrap(), state_dir).is_err());
    assert!(pending.exists());
    assert!(require_no_pending_in(state_dir, state_dir).is_err());
    std::fs::remove_file(pending).unwrap();
    begin_nginx_operation_in(state_dir, state_dir).unwrap();
    assert!(require_no_pending_in(state_dir, state_dir).is_err());
    assert!(begin_nginx_operation_in(state_dir, state_dir).is_err());
    finish_nginx_operation_in(state_dir, state_dir).unwrap();
    require_no_pending_in(state_dir, state_dir).unwrap();
    create_pending(
      dir.path().join(GLOBAL_PENDING).to_str().unwrap(),
      state_dir,
    )
    .unwrap();
    assert!(require_no_pending_in(state_dir, state_dir).is_err());
  }

  #[test]
  fn recovery_preserves_fence_inodes_and_rejects_busy_or_unsafe_markers() {
    let dir = TestDir::new();
    std::fs::set_permissions(
      dir.path(),
      std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    let path = dir.path().to_str().unwrap();
    let global_path = dir.path().join("ncproxy-nginx.fence");
    let local_path = dir.path().join(STATE_START_FENCE);
    let global = open_lock(global_path.to_str().unwrap()).unwrap();
    let local = open_lock(local_path.to_str().unwrap()).unwrap();
    let global_inode = global.metadata().unwrap().ino();
    let local_inode = local.metadata().unwrap().ino();
    assert!(lock_exclusive(&global).unwrap());
    assert!(recover_fences_in(path, path).is_err());
    drop(global);
    drop(local);

    let pending = dir.path().join(GLOBAL_PENDING);
    std::os::unix::fs::symlink("missing", &pending).unwrap();
    assert!(recover_fences_in(path, path).is_err());
    assert!(std::fs::symlink_metadata(&pending).unwrap().is_symlink());
    std::fs::remove_file(&pending).unwrap();

    // An error after global pending is durable stays fail-closed. Once the
    // unsafe local path is corrected, recovery can safely be retried.
    let local_pending = dir.path().join(STATE_PENDING);
    std::os::unix::fs::symlink("missing", &local_pending).unwrap();
    assert!(recover_fences_in(path, path).is_err());
    assert!(pending.is_file());
    assert!(require_no_pending_in(path, path).is_err());
    std::fs::remove_file(local_pending).unwrap();
    recover_fences_in(path, path).unwrap();
    assert_eq!(std::fs::metadata(&global_path).unwrap().ino(), global_inode);
    assert_eq!(std::fs::metadata(&local_path).unwrap().ino(), local_inode);
    assert_eq!(std::fs::read(global_path).unwrap(), FENCE_READY);
    assert_eq!(std::fs::read(local_path).unwrap(), FENCE_READY);
    require_no_pending_in(path, path).unwrap();

    let controllers = controller_locks_in(path, path).unwrap();
    assert!(controller_locks_in(path, path).is_err());
    drop(controllers);
    assert!(controller_locks_in(path, path).is_ok());
  }

  #[ntex::test]
  async fn stale_health_socket_cleanup_preserves_live_and_unrelated_paths() {
    let test_dir = TestDir::new();
    let state_dir = test_dir.path().to_str().unwrap();
    let path = test_dir.path().join("nginx-health.sock");
    let path_str = path.to_str().unwrap();

    // No socket is a normal first start.
    remove_stale_health_socket(state_dir).await.unwrap();
    confirm_health_socket_absent(path_str).unwrap();

    // A listener must never be unlinked merely because its PID file is absent.
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert!(remove_stale_health_socket(state_dir).await.is_err());
    assert!(path.exists());
    drop(listener);

    // Closing a Unix listener leaves the filesystem entry behind.
    remove_stale_health_socket(state_dir).await.unwrap();
    assert!(!path.exists());

    std::fs::write(&path, b"keep").unwrap();
    assert!(remove_stale_health_socket(state_dir).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    std::fs::remove_file(&path).unwrap();

    let target = test_dir.path().join("target");
    std::fs::write(&target, b"keep").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(remove_stale_health_socket(state_dir).await.is_err());
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");

    // ENOENT from connect can also mean a dangling replacement symlink.
    std::fs::remove_file(&target).unwrap();
    assert!(confirm_health_socket_absent(path_str).is_err());
    assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
  }

  #[test]
  fn config_checks_isolate_the_pid_path_from_start_and_reload() {
    const CONF: &str = "/var/lib/ncproxy/nginx.conf";
    for (args, expected) in [
      (&["-t"][..], vec!["-t", "-g", "pid /dev/null;", "-c", CONF]),
      (&[][..], vec!["-g", "pid /run/nginx.pid;", "-c", CONF]),
      (
        &["-s", "reload"][..],
        vec!["-s", "reload", "-g", "pid /run/nginx.pid;", "-c", CONF],
      ),
    ] {
      let command = nginx_command(args, CONF);
      let command = command.as_std();
      assert_eq!(command.get_program(), "nginx");
      let actual = command
        .get_args()
        .map(|arg| arg.to_str().unwrap())
        .collect::<Vec<_>>();
      assert_eq!(actual, expected);
      if args.is_empty() {
        let title = format!("nginx: master process nginx {}", actual.join(" "));
        assert!(is_master_title(title.as_bytes(), CONF));
      }
    }
    let rendered = CONF_TEMPLATE
      .compile(&liquid::object!({
        "nginx_dir": "/etc/nginx",
        "state_dir": "/tmp/ncproxy",
        "health_generation": GENERATION,
      }))
      .unwrap();
    assert!(
      !rendered
        .lines()
        .any(|line| line.trim_start().starts_with("pid "))
    );
  }

  #[ntex::test]
  async fn rejected_transaction_restores_all_previous_configs() {
    let test_dir = TestDir::new();
    let store = Store::new(test_dir.path().to_str().unwrap());
    store
      .write_conf_file("rule", "old site", &NginxRuleKind::Site)
      .await
      .unwrap();
    store
      .write_conf_file("rule", "old stream", &NginxRuleKind::Stream)
      .await
      .unwrap();
    store
      .write_conf_file("shared-upstream", "old upstream", &NginxRuleKind::Site)
      .await
      .unwrap();
    let before = store.snapshot_config(&[]).await.unwrap();

    let result = config_transaction(&store, &[], || async {
      store.replace_rule("rule", Some("new site"), None).await?;
      store
        .write_conf_file(
          "shared-upstream",
          "new upstream",
          &NginxRuleKind::Site,
        )
        .await?;
      Err(IoError::invalid_data("candidate", "rejected"))
    })
    .await;

    assert!(result.is_err());
    assert_eq!(store.snapshot_config(&[]).await.unwrap(), before);
  }

  #[ntex::test]
  async fn successful_update_removes_a_disappeared_rule_kind() {
    let test_dir = TestDir::new();
    let store = Store::new(test_dir.path().to_str().unwrap());
    store
      .write_conf_file("rule", "old site", &NginxRuleKind::Site)
      .await
      .unwrap();
    store
      .write_conf_file("rule", "old stream", &NginxRuleKind::Stream)
      .await
      .unwrap();

    config_transaction(&store, &[], || async {
      store.replace_rule("rule", Some("new site"), None).await
    })
    .await
    .unwrap();

    let snapshot = store.snapshot_config(&[]).await.unwrap();
    assert_eq!(snapshot.sites.get("rule.conf").unwrap(), "new site");
    assert!(!snapshot.streams.contains_key("rule.conf"));
    assert!(!test_dir.path().join("streams-enabled/rule.conf").exists());
  }

  #[ntex::test]
  async fn full_rebuild_removes_configs_absent_from_committed_state() {
    let test_dir = TestDir::new();
    let store = Store::new(test_dir.path().to_str().unwrap());
    store
      .write_conf_file("deleted-rule", "stale", &NginxRuleKind::Site)
      .await
      .unwrap();

    config_transaction(&store, &[], || async {
      store.clear_config().await?;
      store
        .write_conf_file("live-rule", "committed", &NginxRuleKind::Site)
        .await
    })
    .await
    .unwrap();

    let snapshot = store.snapshot_config(&[]).await.unwrap();
    assert!(!snapshot.sites.contains_key("deleted-rule.conf"));
    assert_eq!(snapshot.sites.get("live-rule.conf").unwrap(), "committed");
  }
}
