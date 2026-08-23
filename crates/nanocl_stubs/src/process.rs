use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use bollard_next::{
  container::{
    KillContainerOptions, LogOutput, LogsOptions, Stats, StatsOptions,
  },
  exec::{CreateExecOptions, CreateExecResults},
  service::{
    ContainerInspectResponse, ContainerWaitExitError, ContainerWaitResponse,
  },
};

/// Kind of process (Vm, Job, Cargo)
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProcessKind {
  Vm,
  Job,
  Cargo,
}

/// Implement FromStr for ProcessKind for .parse() method
impl FromStr for ProcessKind {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "vm" => Ok(Self::Vm),
      "job" => Ok(Self::Job),
      "cargo" => Ok(Self::Cargo),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Invalid process kind {s}"),
      )),
    }
  }
}

/// Try to convert a string into a ProcessKind
impl TryFrom<String> for ProcessKind {
  type Error = std::io::Error;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_ref() {
      "vm" => Ok(Self::Vm),
      "job" => Ok(Self::Job),
      "cargo" => Ok(Self::Cargo),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Invalid process kind {value}"),
      )),
    }
  }
}

/// Implement Display for ProcessKind
/// This is used to display the kind of the process
/// in a human readable format
impl std::fmt::Display for ProcessKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      Self::Vm => "vm",
      Self::Job => "job",
      Self::Cargo => "cargo",
    };
    write!(f, "{data}")
  }
}

/// Used to create a new process
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessPartial {
  /// The key of the process
  pub key: String,
  /// Name of the process
  pub name: String,
  /// Kind of the process (Job, Vm, Cargo)
  pub kind: ProcessKind,
  /// The data of the process a ContainerInspect
  pub data: serde_json::Value,
  /// Name of the node where the container is running
  pub node_name: String,
  /// Canonical resource key for cargoes and VMs; jobs use their name
  pub kind_key: String,
  /// The created at date
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub created_at: Option<chrono::NaiveDateTime>,
}

/// Represents a process (Vm, Job, Cargo)
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Process {
  /// The key of the process
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// Last time the instance was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the process
  pub name: String,
  /// Kind of the process (Job, Vm, Cargo)
  pub kind: ProcessKind,
  /// Name of the node where the container is running
  pub node_name: String,
  /// Canonical resource key for cargoes and VMs; jobs use their name
  pub kind_key: String,
  /// The data of the process a ContainerInspect
  pub data: ContainerInspectResponse,
}

/// Options for killing a concrete process
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessKillOptions {
  /// Signal to send to the process; defaults to SIGKILL
  pub signal: String,
}

impl Default for ProcessKillOptions {
  fn default() -> Self {
    Self {
      signal: "SIGKILL".to_owned(),
    }
  }
}

impl From<ProcessKillOptions> for KillContainerOptions<String> {
  fn from(options: ProcessKillOptions) -> Self {
    Self {
      signal: options.signal,
    }
  }
}

/// Options for creating an exec instance in a concrete process
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessExecCreateOptions {
  /// Attach to standard input
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub attach_stdin: Option<bool>,
  /// Attach to standard output
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub attach_stdout: Option<bool>,
  /// Attach to standard error
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub attach_stderr: Option<bool>,
  /// Allocate a pseudo-TTY
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub tty: Option<bool>,
  /// Docker-compatible detach key sequence
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub detach_keys: Option<String>,
  /// Environment variables in `NAME=value` form
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub env: Option<Vec<String>>,
  /// Command and arguments to execute
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cmd: Option<Vec<String>>,
  /// Run the command with extended privileges
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub privileged: Option<bool>,
  /// User and optional group for the command
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub user: Option<String>,
  /// Working directory inside the process
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub working_dir: Option<String>,
}

impl From<ProcessExecCreateOptions> for CreateExecOptions {
  fn from(options: ProcessExecCreateOptions) -> Self {
    Self {
      attach_stdin: options.attach_stdin,
      attach_stdout: options.attach_stdout,
      attach_stderr: options.attach_stderr,
      tty: options.tty,
      detach_keys: options.detach_keys,
      env: options.env,
      cmd: options.cmd,
      privileged: options.privileged,
      user: options.user,
      working_dir: options.working_dir,
    }
  }
}

/// Result of creating a process exec instance
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessExecCreated {
  /// Docker-owned exec instance ID
  pub id: String,
}

impl From<CreateExecResults> for ProcessExecCreated {
  fn from(result: CreateExecResults) -> Self {
    Self { id: result.id }
  }
}

/// Text controls accepted by an attached process exec WebSocket
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum ProcessExecInputControl {
  /// Close the Docker stdin side while continuing to receive output
  StdinEof,
  /// Resize an attached TTY
  Resize { width: u16, height: u16 },
  /// Drop this attachment without stopping the Docker exec
  Detach,
}

/// Text controls emitted by an attached process exec WebSocket
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
pub enum ProcessExecOutputControl {
  /// The exec completed with this exit code
  Exit { code: i64 },
  /// The client attachment was detached
  Detached,
  /// The bridge failed or rejected a protocol message
  Error { message: String },
}

/// Channel marker at the beginning of a server-to-client binary exec frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessExecOutputChannel {
  Stdout = 0x01,
  Stderr = 0x02,
  Console = 0x03,
}

impl ProcessExecOutputChannel {
  pub const fn marker(self) -> u8 {
    self as u8
  }
}

/// Kind of Output
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum OutputKind {
  /// Data is a standard input
  StdIn,
  /// Data is a standard output
  StdOut,
  /// Data is a standard error
  StdErr,
  /// Data is a console output
  Console,
}

/// Output is the output of an exec command
/// It contains the kind of the output and the data
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct OutputLog {
  /// Kind of the output
  pub kind: OutputKind,
  /// Data of the output
  pub data: String,
}

/// Convert a LogOutput into an OutputLog
impl From<LogOutput> for OutputLog {
  fn from(output: LogOutput) -> Self {
    match output {
      LogOutput::StdOut { message } => Self {
        kind: OutputKind::StdOut,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::StdErr { message } => Self {
        kind: OutputKind::StdErr,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::Console { message } => Self {
        kind: OutputKind::Console,
        data: String::from_utf8_lossy(&message).to_string(),
      },
      LogOutput::StdIn { message } => Self {
        kind: OutputKind::StdIn,
        data: String::from_utf8_lossy(&message).to_string(),
      },
    }
  }
}

/// Stream of logs of a process
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessOutputLog {
  pub name: String,
  pub log: OutputLog,
}

/// Log cargo query
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessLogQuery {
  /// Only include logs since unix timestamp
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub until: Option<i64>,
  /// Bool, if set include timestamp to ever log line
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub timestamps: Option<bool>,
  /// Bool, if set open the log as stream
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub follow: Option<bool>,
  /// If integer only return last n logs, if "all" returns all logs
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub tail: Option<String>,
  /// Include stderr in response
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub stderr: Option<bool>,
  /// Include stdout in response
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub stdout: Option<bool>,
}

/// Convert a ProcessLogQuery into a LogsOptions
impl From<ProcessLogQuery> for LogsOptions<String> {
  fn from(query: ProcessLogQuery) -> LogsOptions<String> {
    LogsOptions::<String> {
      follow: query.follow.unwrap_or_default(),
      timestamps: query.timestamps.unwrap_or_default(),
      since: query.since.unwrap_or_default(),
      until: query.until.unwrap_or_default(),
      tail: query.tail.to_owned().unwrap_or("all".to_string()),
      stdout: query.stdout.unwrap_or(true),
      stderr: query.stdout.unwrap_or(true),
    }
  }
}

/// Used to wait for a process to reach a certain state
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum WaitCondition {
  NotRunning,
  #[default]
  NextExit,
  Removed,
}

/// Implement Display for WaitCondition
impl std::fmt::Display for WaitCondition {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      WaitCondition::NextExit => write!(f, "next-exit"),
      WaitCondition::NotRunning => write!(f, "not-running"),
      WaitCondition::Removed => write!(f, "removed"),
    }
  }
}

/// Convert a WaitCondition into a String
impl From<WaitCondition> for std::string::String {
  fn from(value: WaitCondition) -> Self {
    match value {
      WaitCondition::NextExit => "next-exit",
      WaitCondition::NotRunning => "not-running",
      WaitCondition::Removed => "removed",
    }
    .to_owned()
  }
}

/// Implement FromStr for WaitCondition
impl std::str::FromStr for WaitCondition {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_ascii_lowercase().as_str() {
      "next-exit" => Ok(WaitCondition::NextExit),
      "not-running" => Ok(WaitCondition::NotRunning),
      "removed" => Ok(WaitCondition::Removed),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Invalid wait condition",
      )),
    }
  }
}

/// Query for the process wait endpoint
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessWaitQuery {
  // Wait condition
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub condition: Option<WaitCondition>,
}

/// Stream of wait response of a process
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessWaitResponse {
  /// Process name
  pub process_name: String,
  /// Exit code of the container
  pub status_code: i64,
  /// Wait error
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub error: Option<ContainerWaitExitError>,
}

impl ProcessWaitResponse {
  pub fn from_container_wait_response(
    response: ContainerWaitResponse,
    container_name: String,
  ) -> ProcessWaitResponse {
    ProcessWaitResponse {
      process_name: container_name,
      status_code: response.status_code,
      error: response.error,
    }
  }
}

/// Stats process query
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessStatsQuery {
  /// Stream the output. If false, the stats will be output once and then it will disconnect.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub stream: Option<bool>,
  /// Only get a single stat instead of waiting for 2 cycles. Must be used with `stream=false`.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub one_shot: Option<bool>,
}

/// Stats of a process
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct ProcessStats {
  pub name: String,
  pub stats: Stats,
}

impl From<ProcessStatsQuery> for StatsOptions {
  fn from(query: ProcessStatsQuery) -> StatsOptions {
    StatsOptions {
      stream: query.stream.unwrap_or(true),
      one_shot: query.one_shot.unwrap_or_default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use bollard_next::exec::CreateExecOptions;

  use super::{
    ProcessExecCreateOptions, ProcessExecCreated, ProcessExecInputControl,
    ProcessExecOutputControl, ProcessKillOptions,
  };

  #[test]
  fn process_kill_options_preserve_wire_shape() {
    let value = serde_json::to_value(ProcessKillOptions {
      signal: "SIGTERM".to_owned(),
    })
    .expect("ProcessKillOptions must serialize");

    assert_eq!(value, serde_json::json!({ "signal": "SIGTERM" }));
  }

  #[test]
  fn process_exec_create_options_preserve_wire_shape_and_command() {
    let options = ProcessExecCreateOptions {
      attach_stdin: Some(true),
      attach_stdout: Some(true),
      attach_stderr: Some(true),
      tty: Some(true),
      detach_keys: Some("ctrl-p,ctrl-q".to_owned()),
      env: Some(vec!["FOO=bar".to_owned()]),
      cmd: Some(vec!["/bin/sh".to_owned(), "-l".to_owned()]),
      privileged: Some(false),
      user: Some("1000:1000".to_owned()),
      working_dir: Some("/work".to_owned()),
    };

    let value = serde_json::to_value(&options)
      .expect("ProcessExecCreateOptions must serialize");
    assert_eq!(
      value,
      serde_json::json!({
        "AttachStdin": true,
        "AttachStdout": true,
        "AttachStderr": true,
        "Tty": true,
        "DetachKeys": "ctrl-p,ctrl-q",
        "Env": ["FOO=bar"],
        "Cmd": ["/bin/sh", "-l"],
        "Privileged": false,
        "User": "1000:1000",
        "WorkingDir": "/work"
      })
    );

    let docker_options: CreateExecOptions = options.into();
    assert_eq!(docker_options.attach_stdin, Some(true));
    assert_eq!(docker_options.attach_stdout, Some(true));
    assert_eq!(docker_options.attach_stderr, Some(true));
    assert_eq!(docker_options.tty, Some(true));
    assert_eq!(docker_options.detach_keys.as_deref(), Some("ctrl-p,ctrl-q"));
    assert_eq!(docker_options.env, Some(vec!["FOO=bar".to_owned()]));
    assert_eq!(
      docker_options.cmd,
      Some(vec!["/bin/sh".to_owned(), "-l".to_owned()])
    );
    assert_eq!(docker_options.privileged, Some(false));
    assert_eq!(docker_options.user.as_deref(), Some("1000:1000"));
    assert_eq!(docker_options.working_dir.as_deref(), Some("/work"));
  }

  #[test]
  fn process_exec_create_options_keep_absent_values_absent() {
    let options = ProcessExecCreateOptions::default();
    let value = serde_json::to_value(&options)
      .expect("ProcessExecCreateOptions must serialize");
    let docker_options: CreateExecOptions = options.into();

    assert_eq!(value, serde_json::json!({}));
    assert_eq!(docker_options, CreateExecOptions::default());
  }

  #[test]
  fn process_exec_created_preserves_wire_shape() {
    let value = serde_json::to_value(ProcessExecCreated {
      id: "docker-exec-id".to_owned(),
    })
    .expect("ProcessExecCreated must serialize");

    assert_eq!(value, serde_json::json!({ "Id": "docker-exec-id" }));
  }

  #[test]
  fn process_exec_input_controls_parse() {
    assert_eq!(
      serde_json::from_str::<ProcessExecInputControl>(
        r#"{"type":"stdin_eof"}"#
      )
      .unwrap(),
      ProcessExecInputControl::StdinEof
    );
    assert_eq!(
      serde_json::from_str::<ProcessExecInputControl>(
        r#"{"type":"resize","width":80,"height":24}"#
      )
      .unwrap(),
      ProcessExecInputControl::Resize {
        width: 80,
        height: 24,
      }
    );
    assert_eq!(
      serde_json::from_str::<ProcessExecInputControl>(r#"{"type":"detach"}"#)
        .unwrap(),
      ProcessExecInputControl::Detach
    );
  }

  #[test]
  fn process_exec_output_controls_serialize() {
    assert_eq!(
      serde_json::to_value(ProcessExecOutputControl::Exit { code: 42 })
        .unwrap(),
      serde_json::json!({ "type": "exit", "code": 42 })
    );
    assert_eq!(
      serde_json::to_value(ProcessExecOutputControl::Detached).unwrap(),
      serde_json::json!({ "type": "detached" })
    );
    assert_eq!(
      serde_json::to_value(ProcessExecOutputControl::Error {
        message: "bridge failed".to_owned(),
      })
      .unwrap(),
      serde_json::json!({
        "type": "error",
        "message": "bridge failed"
      })
    );
  }
}
