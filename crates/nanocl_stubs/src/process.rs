use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use bollard_next::{
  container::{LogOutput, LogsOptions, Stats, StatsOptions},
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
  /// Key of the related kind
  pub kind_key: String,
  /// The created at date
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
  /// Key of the related kind
  pub kind_key: String,
  /// The data of the process a ContainerInspect
  pub data: ContainerInspectResponse,
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
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Only include logs since unix timestamp
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  pub until: Option<i64>,
  /// Bool, if set include timestamp to ever log line
  pub timestamps: Option<bool>,
  /// Bool, if set open the log as stream
  pub follow: Option<bool>,
  /// If integer only return last n logs, if "all" returns all logs
  pub tail: Option<String>,
  /// Include stderr in response
  pub stderr: Option<bool>,
  /// Include stdout in response
  pub stdout: Option<bool>,
}

impl ProcessLogQuery {
  /// Set namespace of a ProcessLogQuery
  pub fn of_namespace(nsp: &str) -> ProcessLogQuery {
    ProcessLogQuery {
      namespace: Some(nsp.to_owned()),
      since: None,
      until: None,
      timestamps: None,
      follow: None,
      tail: None,
      stderr: None,
      stdout: None,
    }
  }
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
  pub condition: Option<WaitCondition>,
  /// Namespace where belong the process
  pub namespace: Option<String>,
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
  #[serde(skip_serializing_if = "Option::is_none")]
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
  /// Name of the namespace
  pub namespace: Option<String>,
  /// Stream the output. If false, the stats will be output once and then it will disconnect.
  pub stream: Option<bool>,
  /// Only get a single stat instead of waiting for 2 cycles. Must be used with `stream=false`.
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
