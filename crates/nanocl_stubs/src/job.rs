use std::io;
use serde::{Serialize, Deserialize};

use bollard_next::container::Config;
use bollard_next::service::{ContainerWaitExitError, ContainerWaitResponse};

use crate::cargo::OutputLog;
use crate::node::NodeContainerSummary;

/// ## Job
///
/// A job is a collection of containers to run in sequence as a single unit to act like a command
///
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Job {
  /// Name of the job
  pub name: String,
  /// When the job have been created
  pub created_at: chrono::NaiveDateTime,
  /// When the job have been updated
  pub updated_at: chrono::NaiveDateTime,
  /// Secrets to load as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Containers to run
  pub containers: Vec<Config>,
}

/// ## Job summary
///
/// It's the datastructure returned by the list endpoint
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobSummary {
  /// Name of the job
  pub name: String,
  /// Creation date of the job
  pub created_at: chrono::NaiveDateTime,
  /// Last update of the job
  pub updated_at: chrono::NaiveDateTime,
  /// Configuration of the job
  pub config: Job,
  /// Number of instances
  pub instance_total: usize,
  /// Number of instance that succeeded
  pub instance_success: usize,
  /// Number of instance running
  pub instance_running: usize,
  /// Number of instance failed
  pub instance_failed: usize,
}

/// ## Job partial
///
/// Job partial is used to create a new job
///
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct JobPartial {
  /// Name of the job
  pub name: String,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Secrets to load as environment variables
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// List of container to run
  pub containers: Vec<Config>,
}

/// ## Job inspect
///
/// Is a detailed view of a job
/// It also contains the list of instances
///
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobInspect {
  /// Name of the job
  pub name: String,
  /// When the job have been created
  pub created_at: chrono::NaiveDateTime,
  /// When the job have been updated
  pub updated_at: chrono::NaiveDateTime,
  /// Secrets to load as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Metadata (user defined)
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Containers to run
  pub containers: Vec<Config>,
  /// Number of instances
  pub instance_total: usize,
  /// Number of instance that succeeded
  pub instance_success: usize,
  /// Number of instance running
  pub instance_running: usize,
  /// Number of instance failed
  pub instance_failed: usize,
  /// List of containers
  pub instances: Vec<NodeContainerSummary>,
}

impl From<JobInspect> for JobPartial {
  fn from(job: JobInspect) -> Self {
    Self {
      name: job.name,
      secrets: job.secrets,
      metadata: job.metadata,
      containers: job.containers,
    }
  }
}

/// ## Job log output
///
/// Output of a job log
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobLogOutput {
  pub container_name: String,
  pub log: OutputLog,
}

/// # Wait condition
///
/// Wait condition is used to wait for a job to finish or to be removed
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum WaitCondition {
  NotRunning,
  #[default]
  NextExit,
  Removed,
}

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

impl std::str::FromStr for WaitCondition {
  type Err = io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_ascii_lowercase().as_str() {
      "next-exit" => Ok(WaitCondition::NextExit),
      "not-running" => Ok(WaitCondition::NotRunning),
      "removed" => Ok(WaitCondition::Removed),
      _ => Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid wait condition",
      )),
    }
  }
}

/// Job wait
///
/// Query for the wait endpoint
///
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobWaitQuery {
  // Wait condition
  pub condition: Option<WaitCondition>,
}

/// ## Job wait response
///
/// Response of the wait stream
///
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobWaitResponse {
  /// Container id
  pub container_name: String,
  /// Exit code of the container
  pub status_code: i64,
  /// Wait error
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<ContainerWaitExitError>,
}

impl JobWaitResponse {
  pub fn from_container_wait_response(
    response: ContainerWaitResponse,
    container_name: String,
  ) -> JobWaitResponse {
    JobWaitResponse {
      container_name,
      status_code: response.status_code,
      error: response.error,
    }
  }
}
