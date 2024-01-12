use std::str::FromStr;

use diesel::prelude::*;

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

#[derive(Clone, Debug, Default, AsChangeset)]
#[diesel(table_name = object_process_statuses)]
pub struct ObjPsStatusUpdate {
  pub wanted: Option<String>,
  pub prev_wanted: Option<String>,
  pub actual: Option<String>,
  pub prev_actual: Option<String>,
}

#[derive(Debug, Default)]
pub enum ObjPsStatusKind {
  #[default]
  Created,
  Starting,
  Running,
  Patching,
  Deleting,
  Delete,
  Stopped,
  Failed,
  Unknown,
}

impl FromStr for ObjPsStatusKind {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "created" => Ok(Self::Created),
      "starting" => Ok(Self::Starting),
      "running" => Ok(Self::Running),
      "stopped" => Ok(Self::Stopped),
      "failed" => Ok(Self::Failed),
      "deleting" => Ok(Self::Deleting),
      "delete" => Ok(Self::Delete),
      "patching" => Ok(Self::Patching),
      _ => Ok(Self::Unknown),
    }
  }
}

impl ToString for ObjPsStatusKind {
  fn to_string(&self) -> String {
    match self {
      Self::Created => "created",
      Self::Starting => "starting",
      Self::Running => "running",
      Self::Stopped => "stopped",
      Self::Failed => "failed",
      Self::Unknown => "<unknown>",
      Self::Deleting => "deleting",
      Self::Delete => "delete",
      Self::Patching => "patching",
    }
    .to_owned()
  }
}

pub struct ObjPsStatusPartial {
  pub key: String,
  pub wanted: ObjPsStatusKind,
  pub prev_wanted: ObjPsStatusKind,
  pub actual: ObjPsStatusKind,
  pub prev_actual: ObjPsStatusKind,
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
