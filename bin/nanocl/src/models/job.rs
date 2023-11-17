use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::job::{WaitCondition, JobSummary};

use super::DisplayFormat;

/// ## Job wait options
///
/// `nanocl job wait` available options
///
#[derive(Parser)]
pub struct JobWaitOpts {
  /// State to wait
  #[clap(short = 'c')]
  pub condition: Option<WaitCondition>,
  /// Name of job to wait
  pub name: String,
}

/// ## Job list options
///
/// `nanocl job ls` available options
///
#[derive(Parser)]
pub struct JobListOpts {
  /// Only show job names
  #[clap(long, short)]
  pub quiet: bool,
}

/// ## Job remove options
///
/// `nanocl job rm` available options
///
#[derive(Parser)]
pub struct JobRemoveOpts {
  /// Name of job to remove
  pub names: Vec<String>,
  /// Skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
}

/// ## Job inspect options
///
/// `nanocl job inspect` available options
///
#[derive(Parser)]
pub struct JobInspectOpts {
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
  /// Name of job to inspect
  pub name: String,
}

#[derive(Parser)]
pub struct JobLogsOpts {
  /// Name of job to inspect
  pub name: String,
}

/// ## Job command
///
/// `nanocl job` available commands
///
#[derive(Subcommand)]
pub enum JobCommand {
  /// List existing job
  #[clap(alias("ls"))]
  List(JobListOpts),
  /// Remove job by its name
  #[clap(alias("rm"))]
  Remove(JobRemoveOpts),
  /// Inspect a job by its name
  Inspect(JobInspectOpts),
  /// Show logs of a job
  Logs(JobLogsOpts),
  /// Wait for a job to finish
  Wait(JobWaitOpts),
  /// Start a job
  Start(JobStartOpts),
}

/// ## Job start options
///
/// `nanocl job start` available options
///
#[derive(Parser)]
pub struct JobStartOpts {
  /// Name of job to start
  pub name: String,
}

/// ## Job arguments
///
/// `nanocl job` available subcommands
///
#[derive(Parser)]
pub struct JobArg {
  #[clap(subcommand)]
  pub command: JobCommand,
}

/// Job row
///
/// Used to display job information in a table
///
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct JobRow {
  /// Name of the job
  pub name: String,
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
  #[tabled(rename = "CREATED AT")]
  pub updated_at: String,
}

/// Convert [JobSummary](JobSummary) to [JobRow](JobRow)
///
impl From<JobSummary> for JobRow {
  fn from(job: JobSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(job.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(job.updated_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: job.name,
      total: job.instance_total,
      running: job.instance_running,
      succeeded: job.instance_success,
      failed: job.instance_failed,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}
