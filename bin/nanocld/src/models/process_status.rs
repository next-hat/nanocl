use diesel::prelude::*;

use crate::schema::process_statuses;

#[derive(Debug, Clone, Identifiable, Insertable, Queryable)]
#[diesel(primary_key(key))]
#[diesel(table_name = process_statuses)]
pub struct ProcessStatusDb {
  pub key: String,
  pub created_at: chrono::NaiveDateTime,
  pub updated_at: chrono::NaiveDateTime,
  pub current: String,
  pub previous: String,
  pub wanted: String,
}
