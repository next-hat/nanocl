use std::{
  fs,
  io::{Read, Write},
  path::Path,
  thread,
};

#[cfg(not(target_os = "windows"))]
use std::{
  io::IsTerminal,
  os::fd::AsRawFd,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use futures::{StreamExt, channel::mpsc, select};
use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::process::{
  ProcessExecCreateOptions, ProcessExecInputControl, ProcessExecOutputChannel,
  ProcessExecOutputControl,
};
use ntex::{util::Bytes, ws};
#[cfg(not(target_os = "windows"))]
use termios::{TCSANOW, Termios, cfmakeraw, tcsetattr};

use crate::{config::CliConfig, models::ExecOpts};

const DEFAULT_DETACH_KEYS: &str = "ctrl-p,ctrl-q";
const STDIN_BUFFER_SIZE: usize = 8 * 1024;

enum InputAction {
  Data(Vec<u8>),
  Eof,
  Detach,
  Resize { width: u16, height: u16 },
  Error(String),
}

struct DetachScanner {
  sequence: Vec<u8>,
  pending: Vec<u8>,
}

impl DetachScanner {
  fn new(sequence: Vec<u8>) -> Self {
    Self {
      sequence,
      pending: Vec::new(),
    }
  }

  fn scan(&mut self, input: &[u8]) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(input.len());
    for byte in input {
      self.pending.push(*byte);
      while !self.sequence.starts_with(&self.pending) {
        output.push(self.pending.remove(0));
      }
      if self.pending == self.sequence {
        self.pending.clear();
        return (output, true);
      }
    }
    (output, false)
  }

  fn finish(&mut self) -> Vec<u8> {
    std::mem::take(&mut self.pending)
  }
}

fn invalid_input(context: &str, message: impl std::fmt::Display) -> IoError {
  IoError::invalid_input(context.to_owned(), message.to_string())
}

fn validate_env_key(key: &str) -> IoResult<()> {
  if key.is_empty() {
    return Err(invalid_input("Process environment", "empty variable name"));
  }
  Ok(())
}

fn resolve_env_entry_with(
  entry: &str,
  lookup: impl FnOnce(&str) -> Option<String>,
) -> IoResult<(String, String)> {
  if entry.contains('\0') {
    return Err(invalid_input(
      "Process environment",
      "environment entries cannot contain NUL bytes",
    ));
  }
  let (key, value) = match entry.split_once('=') {
    Some((key, _)) => (key, entry.to_owned()),
    None => {
      validate_env_key(entry)?;
      let value = lookup(entry)
        .map(|value| format!("{entry}={value}"))
        .unwrap_or_else(|| entry.to_owned());
      return Ok((entry.to_owned(), value));
    }
  };
  validate_env_key(key)?;
  Ok((key.to_owned(), value))
}

fn upsert_env_entry(
  env: &mut Vec<(String, String)>,
  key: String,
  value: String,
) {
  if let Some((_, current)) = env.iter_mut().find(|(name, _)| name == &key) {
    *current = value;
  } else {
    env.push((key, value));
  }
}

fn apply_env_entry_with(
  env: &mut Vec<(String, String)>,
  entry: &str,
  lookup: impl FnOnce(&str) -> Option<String>,
) -> IoResult<()> {
  let (key, value) = resolve_env_entry_with(entry, lookup)?;
  upsert_env_entry(env, key, value);
  Ok(())
}

fn apply_env_entry(
  env: &mut Vec<(String, String)>,
  entry: &str,
) -> IoResult<()> {
  apply_env_entry_with(env, entry, |key| std::env::var(key).ok())
}

fn apply_env_file_contents_with(
  env: &mut Vec<(String, String)>,
  contents: &str,
  path: &Path,
  mut lookup: impl FnMut(&str) -> Option<String>,
) -> IoResult<()> {
  for (index, line) in contents.lines().enumerate() {
    let line = if index == 0 {
      line.strip_prefix('\u{feff}').unwrap_or(line)
    } else {
      line
    };
    let line = line.trim_start_matches(char::is_whitespace);
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let result = (|| {
      if line.contains('\0') {
        return Err(invalid_input(
          "Process environment",
          "environment entries cannot contain NUL bytes",
        ));
      }
      let (key, value) = match line.split_once('=') {
        Some((key, _)) => (key, Some(line.to_owned())),
        None => (line, lookup(line).map(|value| format!("{line}={value}"))),
      };
      validate_env_key(key)?;
      if key.chars().any(char::is_whitespace) {
        return Err(invalid_input(
          "Process environment",
          format!("variable name `{key}` contains whitespace"),
        ));
      }
      if let Some(value) = value {
        upsert_env_entry(env, key.to_owned(), value);
      }
      Ok(())
    })();
    result.map_err(|err| {
      IoError::invalid_input(
        format!("Environment file {}:{}", path.display(), index + 1),
        err.to_string(),
      )
    })?;
  }
  Ok(())
}

fn apply_env_file_contents(
  env: &mut Vec<(String, String)>,
  contents: &str,
  path: &Path,
) -> IoResult<()> {
  apply_env_file_contents_with(env, contents, path, |key| {
    std::env::var(key).ok()
  })
}

fn resolve_environment(options: &ExecOpts) -> IoResult<Vec<String>> {
  let mut env = Vec::new();
  for path in &options.env_file {
    let contents = fs::read_to_string(path).map_err(|err| {
      IoError::with_context(
        format!("Unable to read environment file {}", path.display()),
        err,
      )
    })?;
    apply_env_file_contents(&mut env, &contents, path)?;
  }
  for entry in &options.env {
    apply_env_entry(&mut env, entry)?;
  }
  Ok(env.into_iter().map(|(_, value)| value).collect())
}

fn parse_detach_keys(value: &str) -> IoResult<Vec<u8>> {
  if value.is_empty() {
    return Err(invalid_input(
      "Detach keys",
      "detach key sequence cannot be empty",
    ));
  }
  value
    .split(',')
    .map(|key| {
      if let Some(control) = key.strip_prefix("ctrl-") {
        let bytes = control.as_bytes();
        if bytes.len() != 1 {
          return Err(invalid_input(
            "Detach keys",
            format!("invalid control key `{key}`"),
          ));
        }
        let byte = bytes[0].to_ascii_lowercase();
        return match byte {
          b'a'..=b'z' => Ok(byte - b'a' + 1),
          b'@' => Ok(0),
          b'[' => Ok(27),
          b'\\' => Ok(28),
          b']' => Ok(29),
          b'^' => Ok(30),
          b'_' => Ok(31),
          _ => Err(invalid_input(
            "Detach keys",
            format!("invalid control key `{key}`"),
          )),
        };
      }
      let bytes = key.as_bytes();
      if bytes.len() == 1 && bytes[0].is_ascii() {
        Ok(bytes[0])
      } else {
        Err(invalid_input("Detach keys", format!("invalid key `{key}`")))
      }
    })
    .collect()
}

fn create_options(
  options: &ExecOpts,
  env: Vec<String>,
) -> ProcessExecCreateOptions {
  let attached = !options.detach;
  ProcessExecCreateOptions {
    attach_stdin: Some(attached && options.interactive),
    attach_stdout: Some(attached),
    attach_stderr: Some(attached),
    tty: Some(options.tty),
    detach_keys: options.detach_keys.clone(),
    env: (!env.is_empty()).then_some(env),
    cmd: Some(options.command.clone()),
    privileged: Some(options.privileged),
    user: options.user.clone(),
    working_dir: options.workdir.clone(),
  }
}

fn control_message(control: &ProcessExecInputControl) -> IoResult<ws::Message> {
  let text = serde_json::to_string(control).map_err(|err| {
    IoError::invalid_data(
      "Process exec control".to_owned(),
      format!("unable to serialize control: {err}"),
    )
  })?;
  Ok(ws::Message::Text(text.into()))
}

async fn send_control(
  sink: &ws::WsSink,
  control: &ProcessExecInputControl,
) -> IoResult<()> {
  sink.send(control_message(control)?).await.map_err(|err| {
    IoError::interrupted(
      "Process exec WebSocket".to_owned(),
      format!("unable to send control: {err}"),
    )
  })
}

fn start_stdin_forwarder(
  sender: mpsc::UnboundedSender<InputAction>,
  detach_keys: Vec<u8>,
) {
  thread::spawn(move || {
    let mut stdin = std::io::stdin().lock();
    let mut scanner = DetachScanner::new(detach_keys);
    let mut buffer = [0_u8; STDIN_BUFFER_SIZE];
    loop {
      match stdin.read(&mut buffer) {
        Ok(0) => {
          let pending = scanner.finish();
          if !pending.is_empty()
            && sender.unbounded_send(InputAction::Data(pending)).is_err()
          {
            return;
          }
          let _ = sender.unbounded_send(InputAction::Eof);
          return;
        }
        Ok(read) => {
          let (data, detached) = scanner.scan(&buffer[..read]);
          if !data.is_empty()
            && sender.unbounded_send(InputAction::Data(data)).is_err()
          {
            return;
          }
          if detached {
            let _ = sender.unbounded_send(InputAction::Detach);
            return;
          }
        }
        Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
        Err(err) => {
          let _ = sender.unbounded_send(InputAction::Error(err.to_string()));
          return;
        }
      }
    }
  });
}

#[cfg(not(target_os = "windows"))]
struct RawTerminalGuard {
  fd: std::os::fd::RawFd,
  original: Termios,
}

#[cfg(not(target_os = "windows"))]
impl RawTerminalGuard {
  fn enter() -> IoResult<Self> {
    let fd = std::io::stdin().as_raw_fd();
    let original = Termios::from_fd(fd)?;
    let mut raw = original;
    cfmakeraw(&mut raw);
    tcsetattr(fd, TCSANOW, &raw)?;
    Ok(Self { fd, original })
  }
}

#[cfg(not(target_os = "windows"))]
impl Drop for RawTerminalGuard {
  fn drop(&mut self) {
    let _ = tcsetattr(self.fd, TCSANOW, &self.original);
  }
}

#[cfg(not(target_os = "windows"))]
static RESIZE_PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_os = "windows"))]
extern "C" fn handle_sigwinch(_: nix::libc::c_int) {
  RESIZE_PENDING.store(true, Ordering::Relaxed);
}

#[cfg(not(target_os = "windows"))]
fn terminal_size() -> IoResult<(u16, u16)> {
  let mut size = std::mem::MaybeUninit::<nix::libc::winsize>::zeroed();
  let result = unsafe {
    nix::libc::ioctl(
      std::io::stdin().as_raw_fd(),
      nix::libc::TIOCGWINSZ,
      size.as_mut_ptr(),
    )
  };
  if result == -1 {
    return Err(IoError::with_context(
      "Unable to read terminal size",
      std::io::Error::last_os_error(),
    ));
  }
  let size = unsafe { size.assume_init() };
  if size.ws_col == 0 || size.ws_row == 0 {
    return Err(IoError::invalid_data(
      "Unable to read terminal size".to_owned(),
      "terminal width and height must be non-zero".to_owned(),
    ));
  }
  Ok((size.ws_col, size.ws_row))
}

#[cfg(not(target_os = "windows"))]
struct ResizeGuard {
  active: Arc<AtomicBool>,
  previous_handler: nix::libc::sighandler_t,
}

#[cfg(not(target_os = "windows"))]
impl ResizeGuard {
  fn start(sender: mpsc::UnboundedSender<InputAction>) -> IoResult<Self> {
    RESIZE_PENDING.store(false, Ordering::Relaxed);
    let previous_handler = unsafe {
      nix::libc::signal(
        nix::libc::SIGWINCH,
        handle_sigwinch as *const () as nix::libc::sighandler_t,
      )
    };
    if previous_handler == nix::libc::SIG_ERR {
      return Err(IoError::with_context(
        "Unable to observe terminal resize",
        std::io::Error::last_os_error(),
      ));
    }
    let active = Arc::new(AtomicBool::new(true));
    let task_active = active.clone();
    ntex::rt::spawn(async move {
      while task_active.load(Ordering::Relaxed) {
        ntex::time::sleep(Duration::from_millis(100)).await;
        if RESIZE_PENDING.swap(false, Ordering::Relaxed)
          && let Ok((width, height)) = terminal_size()
          && sender
            .unbounded_send(InputAction::Resize { width, height })
            .is_err()
        {
          return;
        }
      }
    });
    Ok(Self {
      active,
      previous_handler,
    })
  }
}

#[cfg(not(target_os = "windows"))]
impl Drop for ResizeGuard {
  fn drop(&mut self) {
    self.active.store(false, Ordering::Relaxed);
    unsafe {
      nix::libc::signal(nix::libc::SIGWINCH, self.previous_handler);
    }
  }
}

#[cfg(not(target_os = "windows"))]
fn prepare_terminal(options: &ExecOpts) -> IoResult<Option<(u16, u16)>> {
  if options.detach {
    return Ok(None);
  }
  let stdin_is_terminal = std::io::stdin().is_terminal();
  if options.tty && options.interactive && !stdin_is_terminal {
    return Err(invalid_input(
      "Process exec terminal",
      "--tty with --interactive requires terminal stdin",
    ));
  }
  if options.tty && stdin_is_terminal {
    Ok(Some(terminal_size()?))
  } else {
    Ok(None)
  }
}

#[cfg(target_os = "windows")]
fn prepare_terminal(options: &ExecOpts) -> IoResult<Option<(u16, u16)>> {
  if !options.detach && options.tty && options.interactive {
    return Err(invalid_input(
      "Process exec terminal",
      "--tty is not supported on Windows",
    ));
  }
  Ok(None)
}

fn write_output(frame: &[u8]) -> IoResult<()> {
  let Some((&channel, data)) = frame.split_first() else {
    return Err(IoError::invalid_data(
      "Process exec output".to_owned(),
      "received an empty binary frame".to_owned(),
    ));
  };
  match channel {
    marker if marker == ProcessExecOutputChannel::Stdout.marker() => {
      let mut stdout = std::io::stdout().lock();
      stdout.write_all(data)?;
      stdout.flush()?;
    }
    marker if marker == ProcessExecOutputChannel::Stderr.marker() => {
      let mut stderr = std::io::stderr().lock();
      stderr.write_all(data)?;
      stderr.flush()?;
    }
    marker if marker == ProcessExecOutputChannel::Console.marker() => {
      let mut stdout = std::io::stdout().lock();
      stdout.write_all(data)?;
      stdout.flush()?;
    }
    marker => {
      return Err(IoError::invalid_data(
        "Process exec output".to_owned(),
        format!("unknown stream marker {marker:#04x}"),
      ));
    }
  }
  Ok(())
}

/// Execute a command in one concrete Docker-backed process.
pub async fn exec_process_command(
  cli_conf: &CliConfig,
  options: &ExecOpts,
) -> IoResult<i32> {
  let env = resolve_environment(options)?;
  let initial_size = prepare_terminal(options)?;
  let detach_keys = if !options.detach && options.interactive {
    Some(parse_detach_keys(
      options
        .detach_keys
        .as_deref()
        .unwrap_or(DEFAULT_DETACH_KEYS),
    )?)
  } else {
    None
  };
  let create_options = create_options(options, env);
  let created = cli_conf
    .client
    .create_process_exec(&options.process, &create_options)
    .await?;

  if options.detach {
    cli_conf
      .client
      .start_process_exec_detached(&created.id)
      .await?;
    return Ok(0);
  }

  let connection = cli_conf
    .client
    .start_process_exec_attached(&created.id)
    .await?;
  let sink = connection.sink();

  #[cfg(not(target_os = "windows"))]
  let _terminal_guard = if options.tty && options.interactive {
    Some(RawTerminalGuard::enter()?)
  } else {
    None
  };

  let (input_sender, mut input_receiver) = mpsc::unbounded();
  if options.interactive {
    start_stdin_forwarder(
      input_sender.clone(),
      detach_keys.expect("interactive exec must have detach keys"),
    );
  }

  #[cfg(not(target_os = "windows"))]
  let _resize_guard = if let Some((width, height)) = initial_size {
    send_control(&sink, &ProcessExecInputControl::Resize { width, height })
      .await?;
    Some(ResizeGuard::start(input_sender.clone())?)
  } else {
    None
  };

  drop(input_sender);
  let mut input_open = options.interactive || initial_size.is_some();
  let mut detach_requested = false;
  let mut frames = connection.seal().receiver();

  loop {
    enum Event<T> {
      Frame(T),
      Input(Option<InputAction>),
    }

    let event = if input_open {
      let frame = frames.next();
      let input = input_receiver.next();
      futures::pin_mut!(frame, input);
      select! {
        frame = frame => Event::Frame(frame),
        input = input => Event::Input(input),
      }
    } else {
      Event::Frame(frames.next().await)
    };

    match event {
      Event::Input(Some(InputAction::Data(data))) => {
        sink
          .send(ws::Message::Binary(Bytes::from(data)))
          .await
          .map_err(|err| {
            IoError::interrupted(
              "Process exec WebSocket".to_owned(),
              format!("unable to send stdin: {err}"),
            )
          })?;
      }
      Event::Input(Some(InputAction::Eof)) => {
        send_control(&sink, &ProcessExecInputControl::StdinEof).await?;
      }
      Event::Input(Some(InputAction::Detach)) => {
        send_control(&sink, &ProcessExecInputControl::Detach).await?;
        detach_requested = true;
      }
      Event::Input(Some(InputAction::Resize { width, height })) => {
        send_control(&sink, &ProcessExecInputControl::Resize { width, height })
          .await?;
      }
      Event::Input(Some(InputAction::Error(message))) => {
        return Err(IoError::interrupted(
          "Process exec stdin".to_owned(),
          message,
        ));
      }
      Event::Input(None) => input_open = false,
      Event::Frame(Some(Ok(ws::Frame::Binary(frame)))) => {
        write_output(&frame)?;
      }
      Event::Frame(Some(Ok(ws::Frame::Text(frame)))) => {
        let control = serde_json::from_slice::<ProcessExecOutputControl>(
          &frame,
        )
        .map_err(|err| {
          IoError::invalid_data(
            "Process exec control".to_owned(),
            format!("unable to parse server control: {err}"),
          )
        })?;
        match control {
          ProcessExecOutputControl::Exit { code } => {
            let code = u8::try_from(code).map_err(|_| {
              IoError::invalid_data(
                "Process exec exit".to_owned(),
                format!("invalid exit code {code}"),
              )
            })?;
            return Ok(i32::from(code));
          }
          ProcessExecOutputControl::Detached if detach_requested => {
            return Ok(0);
          }
          ProcessExecOutputControl::Detached => {
            return Err(IoError::invalid_data(
              "Process exec control".to_owned(),
              "received an unexpected detached acknowledgement".to_owned(),
            ));
          }
          ProcessExecOutputControl::Error { message } => {
            return Err(IoError::other("Process exec".to_owned(), message));
          }
        }
      }
      Event::Frame(Some(Ok(ws::Frame::Ping(data)))) => {
        sink.send(ws::Message::Pong(data)).await.map_err(|err| {
          IoError::interrupted(
            "Process exec WebSocket".to_owned(),
            format!("unable to send pong: {err}"),
          )
        })?;
      }
      Event::Frame(Some(Ok(ws::Frame::Pong(_)))) => {}
      Event::Frame(Some(Ok(ws::Frame::Close(_)))) | Event::Frame(None) => {
        return Err(IoError::interrupted(
          "Process exec WebSocket".to_owned(),
          "connection closed before exit or detach acknowledgement".to_owned(),
        ));
      }
      Event::Frame(Some(Ok(ws::Frame::Continuation(_)))) => {
        return Err(IoError::invalid_data(
          "Process exec WebSocket".to_owned(),
          "fragmented frames are not supported by the exec protocol".to_owned(),
        ));
      }
      Event::Frame(Some(Err(err))) => {
        return Err(IoError::interrupted(
          "Process exec WebSocket".to_owned(),
          format!("transport error: {err}"),
        ));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn exec_options(detach: bool, interactive: bool) -> ExecOpts {
    ExecOpts {
      detach,
      detach_keys: Some("ctrl-a,x".to_owned()),
      env: Vec::new(),
      env_file: Vec::new(),
      interactive,
      privileged: true,
      tty: true,
      user: Some("1000:1000".to_owned()),
      workdir: Some("/work".to_owned()),
      process: "process-id".to_owned(),
      command: vec!["echo".to_owned(), "hello".to_owned()],
    }
  }

  #[test]
  fn create_options_map_attached_and_detached_streams() {
    let attached =
      create_options(&exec_options(false, true), vec!["FOO=bar".to_owned()]);
    assert_eq!(attached.attach_stdin, Some(true));
    assert_eq!(attached.attach_stdout, Some(true));
    assert_eq!(attached.attach_stderr, Some(true));
    assert_eq!(attached.tty, Some(true));
    assert_eq!(attached.detach_keys.as_deref(), Some("ctrl-a,x"));
    assert_eq!(attached.env, Some(vec!["FOO=bar".to_owned()]));
    assert_eq!(attached.cmd, Some(vec!["echo".into(), "hello".into()]));
    assert_eq!(attached.privileged, Some(true));
    assert_eq!(attached.user.as_deref(), Some("1000:1000"));
    assert_eq!(attached.working_dir.as_deref(), Some("/work"));

    let detached = create_options(&exec_options(true, true), Vec::new());
    assert_eq!(detached.attach_stdin, Some(false));
    assert_eq!(detached.attach_stdout, Some(false));
    assert_eq!(detached.attach_stderr, Some(false));
    assert_eq!(detached.env, None);

    let non_interactive =
      create_options(&exec_options(false, false), Vec::new());
    assert_eq!(non_interactive.attach_stdin, Some(false));
    assert_eq!(non_interactive.attach_stdout, Some(true));
    assert_eq!(non_interactive.attach_stderr, Some(true));
  }

  #[test]
  fn env_files_then_direct_entries_override_in_stable_order() {
    let mut env = Vec::new();
    apply_env_file_contents_with(
      &mut env,
      "\u{feff}  # first file\n  FOO=one\nBARE\nEMPTY=\n\n",
      Path::new("first.env"),
      |key| (key == "BARE").then(|| "local".to_owned()),
    )
    .unwrap();
    apply_env_file_contents_with(
      &mut env,
      "FOO=two\nMISSING\nSECOND=value # literal\n",
      Path::new("second.env"),
      |_| None,
    )
    .unwrap();
    apply_env_entry_with(&mut env, "FOO=direct", |_| None).unwrap();
    assert_eq!(
      env,
      vec![
        ("FOO".to_owned(), "FOO=direct".to_owned()),
        ("BARE".to_owned(), "BARE=local".to_owned()),
        ("EMPTY".to_owned(), "EMPTY=".to_owned()),
        ("SECOND".to_owned(), "SECOND=value # literal".to_owned()),
      ]
    );
    assert_eq!(
      resolve_env_entry_with("LOCAL", |key| {
        (key == "LOCAL").then(|| "value".to_owned())
      })
      .unwrap(),
      ("LOCAL".to_owned(), "LOCAL=value".to_owned())
    );
  }

  #[test]
  fn malformed_environment_entries_are_rejected() {
    assert!(resolve_env_entry_with("=value", |_| None).is_err());
    assert!(resolve_env_entry_with("BAD\0KEY=value", |_| None).is_err());
    let mut env = Vec::new();
    assert!(
      apply_env_file_contents_with(
        &mut env,
        "BAD KEY=value\n",
        Path::new("invalid.env"),
        |_| None,
      )
      .is_err()
    );
    assert_eq!(
      resolve_env_entry_with("MISSING", |_| None).unwrap(),
      ("MISSING".to_owned(), "MISSING".to_owned())
    );
  }

  #[test]
  fn detach_keys_parse_and_scan_across_input_chunks() {
    let keys = parse_detach_keys("ctrl-p,ctrl-q").unwrap();
    assert_eq!(keys, vec![0x10, 0x11]);
    let mut scanner = DetachScanner::new(keys);
    assert_eq!(scanner.scan(b"hello\x10"), (b"hello".to_vec(), false));
    assert_eq!(scanner.scan(b"x\x10"), (b"\x10x".to_vec(), false));
    assert_eq!(scanner.scan(b"\x11ignored"), (Vec::new(), true));
    assert!(parse_detach_keys("ctrl-nope").is_err());
    assert!(parse_detach_keys("").is_err());
  }
}
