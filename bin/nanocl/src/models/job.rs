use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::{job::JobSummary, process::WaitCondition};

use super::{
  GenericInspectOpts, GenericListOpts, GenericRemoveOpts, GenericStartOpts,
};

/// `nanocl job wait` available options
#[derive(Clone, Parser)]
pub struct JobWaitOpts {
  /// State to wait
  #[clap(short = 'c')]
  pub condition: Option<WaitCondition>,
  /// Name of job to wait
  pub name: String,
}

/// `nanocl job logs` available options
#[derive(Clone, Parser)]
pub struct JobLogsOpts {
  /// Name of job to show logs
  pub name: String,
  /// Only include logs since unix timestamp
  #[clap(short = 's')]
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  #[clap(short = 'u')]
  pub until: Option<i64>,
  /// If integer only return last n logs, if "all" returns all logs
  #[clap(short = 't')]
  pub tail: Option<String>,
  /// Bool, if set include timestamp to ever log line
  #[clap(long = "timestamps")]
  pub timestamps: bool,
  /// Bool, if set open the log as stream
  #[clap(short = 'f')]
  pub follow: bool,
}

/// `nanocl job` available commands
#[derive(Clone, Subcommand)]
pub enum JobCommand {
  /// List existing job
  #[clap(alias("ls"))]
  List(GenericListOpts),
  /// Remove job by its name
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts),
  /// Inspect a job by its name
  Inspect(GenericInspectOpts),
  /// Show logs of a job
  Logs(JobLogsOpts),
  /// Wait for a job to finish
  Wait(JobWaitOpts),
  /// Start a job
  Start(GenericStartOpts),
}

/// `nanocl job` available subcommands
#[derive(Clone, Parser)]
pub struct JobArg {
  #[clap(subcommand)]
  pub command: JobCommand,
}

/// A job row to display job information in a table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct JobRow {
  /// Name of the job
  pub name: String,
  /// Status of the job
  pub status: String,
  /// Total number of instances
  pub total: usize,
  /// Number of running instances
  pub running: usize,
  /// Number of succeeded instances
  pub succeeded: usize,
  /// Number of failed instances
  pub failed: usize,
  /// When the job was created
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  /// When the job was last updated
  #[tabled(rename = "UPDATED AT")]
  pub updated_at: String,
  /// Age since the job was created, used in compact output
  #[tabled(skip)]
  pub age: String,
}

/// A compact row of the job table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct JobCompactRow {
  pub name: String,
  pub status: String,
  pub total: usize,
  pub running: usize,
  pub succeeded: usize,
  pub failed: usize,
  pub age: String,
}

impl From<JobRow> for JobCompactRow {
  fn from(row: JobRow) -> Self {
    Self {
      name: row.name,
      status: row.status,
      total: row.total,
      running: row.running,
      succeeded: row.succeeded,
      failed: row.failed,
      age: row.age,
    }
  }
}

/// Convert [JobSummary](JobSummary) to [JobRow](JobRow)
impl From<JobSummary> for JobRow {
  fn from(job: JobSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(job.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(job.updated_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: job.spec.name,
      status: format!("{}/{}", job.status.actual, job.status.wanted),
      total: job.instance_total,
      running: job.instance_running,
      succeeded: job.instance_success,
      failed: job.instance_failed,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
      age: super::format_age(
        Some(&job.created_at.and_utc()),
        chrono::Utc::now(),
      ),
    }
  }
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::{job::JobSpec, system::ObjPsStatus};

  use super::*;

  #[test]
  fn compact_job_row_preserves_outcome_counts_and_age() {
    let created_at =
      (chrono::Utc::now() - chrono::Duration::hours(49)).naive_utc();
    let status = ObjPsStatus::default();
    let expected_status = format!("{}/{}", status.actual, status.wanted);
    let row = JobRow::from(JobSummary {
      created_at,
      updated_at: chrono::Utc::now().naive_utc(),
      status,
      instance_total: 4,
      instance_success: 2,
      instance_running: 1,
      instance_failed: 1,
      spec: JobSpec {
        name: "backup".to_owned(),
        ..Default::default()
      },
    });
    assert_eq!(row.age, "2d");
    assert!(!row.created_at.is_empty());
    assert!(!row.updated_at.is_empty());
    assert_eq!(
      JobRow::headers(),
      [
        "NAME",
        "STATUS",
        "TOTAL",
        "RUNNING",
        "SUCCEEDED",
        "FAILED",
        "CREATED AT",
        "UPDATED AT"
      ]
    );
    let compact = JobCompactRow::from(row);
    assert_eq!(compact.name, "backup");
    assert_eq!(compact.status, expected_status);
    assert_eq!(compact.total, 4);
    assert_eq!(compact.running, 1);
    assert_eq!(compact.succeeded, 2);
    assert_eq!(compact.failed, 1);
    assert_eq!(compact.age, "2d");
    assert_eq!(
      JobCompactRow::headers(),
      [
        "NAME",
        "STATUS",
        "TOTAL",
        "RUNNING",
        "SUCCEEDED",
        "FAILED",
        "AGE"
      ]
    );
  }
}
