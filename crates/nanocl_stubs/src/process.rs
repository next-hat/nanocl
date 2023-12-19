#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use bollard_next::service::ContainerInspectResponse;
use bollard_next::container::{LogOutput, LogsOptions, ListContainersOptions};

#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProcessKind {
  Vm,
  Job,
  Cargo,
}

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

impl ToString for ProcessKind {
  fn to_string(&self) -> String {
    match self {
      Self::Vm => "vm",
      Self::Job => "job",
      Self::Cargo => "cargo",
    }
    .to_owned()
  }
}

/// Used to create a new process
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
  /// Key of the node where the container is running
  pub node_key: String,
  /// Key of the related kind
  pub kind_key: String,
  /// The created at date
  pub created_at: Option<chrono::NaiveDateTime>,
}

/// Represents a process (Vm, Job, Cargo)
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
  /// Key of the node where the container is running
  pub node_key: String,
  /// Key of the related kind
  pub kind_key: String,
  /// The data of the process a ContainerInspect
  pub data: ContainerInspectResponse,
}

/// Kind of Output
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct OutputLog {
  /// Kind of the output
  pub kind: OutputKind,
  /// Data of the output
  pub data: String,
}

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

/// Query parameters for the process list endpoint.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProccessQuery {
  /// Return container from all nodes
  pub all: bool,
  /// Return this number of most recently created containers
  pub last: Option<isize>,
  /// Show all containers running for the given namespace
  pub namespace: Option<String>,
}

impl From<ProccessQuery> for ListContainersOptions<String> {
  fn from(query: ProccessQuery) -> Self {
    ListContainersOptions {
      all: query.all,
      limit: query.last,
      ..Default::default()
    }
  }
}
