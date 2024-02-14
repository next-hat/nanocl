use diesel::prelude::*;

use nanocl_error::io::IoError;
use nanocl_stubs::system::{ObjPsStatusPartial, ObjPsStatus};

use crate::schema::object_process_statuses;

#[derive(Debug, Clone, Identifiable, Insertable, Queryable)]
#[diesel(primary_key(key))]
#[diesel(table_name = object_process_statuses)]
pub struct ObjPsStatusDb {
  pub key: String,
  pub created_at: chrono::NaiveDateTime,
  pub updated_at: chrono::NaiveDateTime,
  pub wanted: String,
  pub prev_wanted: String,
  pub actual: String,
  pub prev_actual: String,
}

impl TryFrom<ObjPsStatusDb> for ObjPsStatus {
  type Error = IoError;

  fn try_from(value: ObjPsStatusDb) -> Result<Self, Self::Error> {
    Ok(Self {
      updated_at: value.updated_at,
      wanted: value.wanted.parse()?,
      prev_wanted: value.prev_wanted.parse()?,
      actual: value.actual.parse()?,
      prev_actual: value.prev_actual.parse()?,
    })
  }
}

#[derive(Clone, Debug, Default, AsChangeset)]
#[diesel(table_name = object_process_statuses)]
pub struct ObjPsStatusUpdate {
  pub wanted: Option<String>,
  pub prev_wanted: Option<String>,
  pub actual: Option<String>,
  pub prev_actual: Option<String>,
}

impl From<ObjPsStatusPartial> for ObjPsStatusDb {
  fn from(partial: ObjPsStatusPartial) -> Self {
    Self {
      key: partial.key,
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      wanted: partial.wanted.to_string(),
      prev_wanted: partial.prev_wanted.to_string(),
      actual: partial.actual.to_string(),
      prev_actual: partial.prev_actual.to_string(),
    }
  }
}
