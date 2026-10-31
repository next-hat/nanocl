use diesel::prelude::*;
use nanocl_stubs::job::Job;

use super::OperationDb;
use crate::schema::{job_run_steps, job_runs};

/// Retained workload ownership is independent of the current control operation.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = job_runs)]
pub(crate) struct JobRunDb {
  pub key: uuid::Uuid,
  pub job_name: String,
  pub incarnation_key: uuid::Uuid,
  pub spec_key: uuid::Uuid,
  pub operation_key: uuid::Uuid,
  pub accepted_at: chrono::NaiveDateTime,
  pub started_at: Option<chrono::NaiveDateTime>,
  pub finished_at: Option<chrono::NaiveDateTime>,
  pub state: String,
  pub reason: Option<String>,
  pub cancellation_requested: bool,
}

#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(run_key, step_index))]
#[diesel(table_name = job_run_steps)]
pub(crate) struct JobRunStepDb {
  pub run_key: uuid::Uuid,
  pub step_index: i32,
  pub container_id: Option<String>,
  pub state: String,
  pub started_at: Option<chrono::NaiveDateTime>,
  pub finished_at: Option<chrono::NaiveDateTime>,
  pub exit_code: Option<i64>,
  pub error: Option<String>,
}

pub(crate) struct AcceptedJobRun {
  pub job: Job,
  pub operation: OperationDb,
  pub run: JobRunDb,
}

pub(crate) struct AcceptedJobControl {
  pub job: Job,
  pub operation: OperationDb,
  pub run: Option<JobRunDb>,
}
