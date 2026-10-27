use diesel::prelude::*;

use crate::schema::jobs;

/// This structure represent a job to run.
/// It will create and run a list of containers.
#[derive(Clone, Debug, Queryable, Identifiable, Insertable)]
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
  /// Immutable authored definition.
  pub spec_key: uuid::Uuid,
  /// Retained identity, distinct for each same-name recreation.
  pub incarnation_key: uuid::Uuid,
  /// Most recently accepted execution, including its retained terminal state.
  pub current_run_key: Option<uuid::Uuid>,
}

/// This structure represent the update of a job.
/// It will update the job with the new data.
#[derive(Clone, AsChangeset)]
#[diesel(table_name = jobs)]
pub struct JobUpdateDb {
  pub updated_at: Option<chrono::NaiveDateTime>,
}
