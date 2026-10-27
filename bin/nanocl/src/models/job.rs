use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::{
  job::{JobRun, JobSummary},
  process::WaitCondition,
};

use super::{
  DisplayFormat, GenericInspectOpts, GenericListOpts, GenericRemoveOpts,
  GenericStartOpts, GenericStopOpts,
};

/// `nanocl job history` lists retained executions or inspects a single run.
#[derive(Clone, Parser)]
pub struct JobHistoryOpts {
  /// Job name, including a removed Job
  pub name: String,
  /// Inspect a single execution by its run ID
  #[clap(long, conflicts_with_all = ["limit", "offset"])]
  pub run: Option<String>,
  /// Page size, 1 to 100; defaults to 50
  #[clap(long, value_parser = clap::value_parser!(i64).range(1..=100))]
  pub limit: Option<i64>,
  /// Number of executions to skip, newest first
  #[clap(long, value_parser = clap::value_parser!(i64).range(0..))]
  pub offset: Option<i64>,
  /// Display format; list defaults to a table, detail to configured format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
}

/// One retained execution, including its incarnation to distinguish recreation.
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct JobRunRow {
  pub run: String,
  pub incarnation: String,
  pub state: String,
  #[tabled(rename = "ACCEPTED AT")]
  pub accepted_at: String,
  #[tabled(rename = "STARTED AT")]
  pub started_at: String,
  #[tabled(rename = "FINISHED AT")]
  pub finished_at: String,
  pub reason: String,
}

impl From<JobRun> for JobRunRow {
  fn from(run: JobRun) -> Self {
    let timestamp = |value: chrono::NaiveDateTime| {
      value
        .and_utc()
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
    };
    Self {
      run: run.key.to_string(),
      incarnation: run.incarnation_key.to_string(),
      state: format!("{:?}", run.state),
      accepted_at: timestamp(run.accepted_at),
      started_at: run.started_at.map(timestamp).unwrap_or_else(|| "-".into()),
      finished_at: run.finished_at.map(timestamp).unwrap_or_else(|| "-".into()),
      reason: run.reason.unwrap_or_default(),
    }
  }
}

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
  /// List retained executions or inspect a run; Docker logs are not archived
  History(JobHistoryOpts),
  /// Show logs of a job
  Logs(JobLogsOpts),
  /// Wait for a job to finish
  Wait(JobWaitOpts),
  /// Start a job
  Start(GenericStartOpts),
  /// Stop the current Job execution
  Stop(GenericStopOpts),
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
      incarnation_key: uuid::Uuid::nil(),
      spec_key: uuid::Uuid::nil(),
      current_run_key: None,
      created_at,
      updated_at: chrono::Utc::now().naive_utc(),
      status,
      instance_total: 4,
      instance_success: 2,
      instance_running: 1,
      instance_failed: 1,
      spec: JobSpec {
        key: uuid::Uuid::nil(),
        job_key: "backup".to_owned(),
        version: "v0.19".to_owned(),
        created_at,
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

  #[test]
  fn job_history_parses_bounded_pages_and_run_detail() {
    let args = JobArg::try_parse_from([
      "job",
      "history",
      "backup",
      "--limit",
      "100",
      "--offset",
      "2",
      "--display",
      "json",
    ])
    .unwrap();
    let JobCommand::History(options) = args.command else {
      panic!("expected history command");
    };
    assert_eq!(options.name, "backup");
    assert_eq!(options.limit, Some(100));
    assert_eq!(options.offset, Some(2));
    assert!(matches!(options.display, Some(DisplayFormat::Json)));
    let run = "00000000-0000-0000-0000-000000000001";
    let args =
      JobArg::try_parse_from(["job", "history", "backup", "--run", run])
        .unwrap();
    let JobCommand::History(options) = args.command else {
      panic!("expected history command");
    };
    assert_eq!(options.run.as_deref(), Some(run));
    for invalid in [
      vec!["job", "history", "backup", "--limit", "0"],
      vec!["job", "history", "backup", "--limit", "101"],
      vec!["job", "history", "backup", "--offset", "-1"],
      vec!["job", "history", "backup", "--run", run, "--limit", "1"],
    ] {
      assert!(JobArg::try_parse_from(invalid).is_err());
    }
    assert!(matches!(
      JobArg::try_parse_from(["job", "stop", "backup"])
        .unwrap()
        .command,
      JobCommand::Stop(_)
    ));
  }

  #[test]
  fn job_history_row_preserves_identity_and_unknown_timestamps() {
    let run = JobRun {
      key: uuid::Uuid::from_u128(1),
      job_name: "backup".into(),
      incarnation_key: uuid::Uuid::from_u128(2),
      spec_key: uuid::Uuid::from_u128(3),
      operation_key: uuid::Uuid::from_u128(4),
      accepted_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      started_at: None,
      finished_at: None,
      state: nanocld_client::stubs::job::JobRunState::Unknown,
      reason: Some("Runtime observation unavailable".into()),
    };
    let row = JobRunRow::from(run.clone());
    assert_eq!(row.run, run.key.to_string());
    assert_eq!(row.incarnation, run.incarnation_key.to_string());
    assert_eq!(row.state, "Unknown");
    assert_eq!(row.started_at, "-");
    assert_eq!(row.finished_at, "-");
    assert_eq!(row.reason, "Runtime observation unavailable");
  }
}
