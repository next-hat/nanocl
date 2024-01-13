use diesel::prelude::*;

use crate::schema::jobs;

/// This structure represent a job to run.
/// It will create and run a list of containers.
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
  /// The status key
  pub status_key: String,
  /// The spec
  pub data: serde_json::Value,
  /// The metadata
  pub metadata: Option<serde_json::Value>,
}

/// This structure represent the update of a job.
/// It will update the job with the new data.
#[derive(Clone, AsChangeset)]
#[diesel(table_name = jobs)]
pub struct JobUpdateDb {
  pub updated_at: Option<chrono::NaiveDateTime>,
}
