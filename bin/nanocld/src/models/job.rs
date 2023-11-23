use nanocl_error::io::{IoResult, FromIo};

use nanocl_stubs::job::{Job, JobPartial};

use crate::schema::jobs;

/// ## JobDb
///
/// This structure represent a job to run.
/// It will create and run a list of containers.
///
#[derive(Clone, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = jobs)]
pub struct JobDb {
  /// The key of the job generated with the name
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The updated at data
  pub updated_at: chrono::NaiveDateTime,
  /// The config
  pub data: serde_json::Value,
  /// The metadata
  pub metadata: Option<serde_json::Value>,
}

impl JobDb {
  pub fn into_job(self, config: &JobPartial) -> Job {
    Job {
      name: self.key.clone(),
      created_at: self.created_at,
      updated_at: self.updated_at,
      secrets: config.secrets.clone(),
      metadata: self.metadata.clone(),
      containers: config.containers.clone(),
      schedule: config.schedule.clone(),
    }
  }

  pub fn serialize_data(&self) -> IoResult<JobPartial> {
    Ok(
      serde_json::from_value(self.data.clone())
        .map_err(|err| err.map_err_context(|| "Job"))?,
    )
  }
}

/// ## JobUpdateDb
///
/// This structure represent the update of a job.
/// It will update the job with the new data.
///
#[derive(Clone, AsChangeset)]
#[diesel(table_name = jobs)]
pub struct JobUpdateDb {
  pub updated_at: Option<chrono::NaiveDateTime>,
}
