use crate::schema::jobs;

/// ## JobDbModel
///
/// This structure represent a job to run.
/// It will create and run a list of containers.
///
#[derive(Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = jobs)]
pub struct JobDbModel {
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

/// ## JobDbModelUpdate
///
/// This structure is used to update a job in the database.
///
#[derive(Debug, Default, AsChangeset)]
#[diesel(table_name = jobs)]
pub struct JobUpdateDbModel {
  /// The key of the job generated with the name
  pub key: Option<String>,
  /// The created at date
  pub created_at: Option<chrono::NaiveDateTime>,
  /// The updated at data
  pub updated_at: Option<chrono::NaiveDateTime>,
  /// The config
  pub data: Option<serde_json::Value>,
  /// The metadata
  pub metadata: Option<serde_json::Value>,
}
