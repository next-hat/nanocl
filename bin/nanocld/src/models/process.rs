use diesel::prelude::*;

use nanocl_error::io::{FromIo, IoError};

use nanocl_stubs::process::{Process, ProcessKind, ProcessPartial};

use crate::schema::processes;

/// Represents a process (job, cargo, vm) in the database
#[derive(Clone, Queryable, Identifiable, Insertable, Selectable)]
#[diesel(primary_key(key))]
#[diesel(table_name = processes)]
pub struct ProcessDb {
  /// The key of the process
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// Last time the instance was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the process
  pub name: String,
  /// Kind of the process (Job, Vm, Cargo)
  pub kind: String,
  /// The data of the process a ContainerInspect
  pub data: serde_json::Value,
  /// Id of the node where the container is running
  pub node_name: String,
  /// Id of the related kind
  pub kind_key: String,
}

/// Used to update a process
#[derive(Clone, Default, AsChangeset)]
#[diesel(table_name = processes)]
pub struct ProcessUpdateDb {
  pub key: Option<String>,
  /// Last time the instance was updated
  pub updated_at: Option<chrono::NaiveDateTime>,
  /// Name of instance
  pub name: Option<String>,
  // The updated at data
  pub data: Option<serde_json::Value>,
}

impl TryFrom<ProcessDb> for Process {
  type Error = IoError;

  fn try_from(model: ProcessDb) -> Result<Self, Self::Error> {
    Ok(Self {
      key: model.key,
      created_at: model.created_at,
      updated_at: model.updated_at,
      name: model.name,
      kind: ProcessKind::try_from(model.kind)?,
      data: serde_json::from_value(model.data)
        .map_err(|err| err.map_err_context(|| "Process"))?,
      node_name: model.node_name,
      kind_key: model.kind_key,
    })
  }
}

impl From<&ProcessPartial> for ProcessDb {
  fn from(model: &ProcessPartial) -> Self {
    Self {
      key: model.key.clone(),
      name: model.name.clone(),
      kind: model.kind.to_string(),
      data: model.data.clone(),
      node_name: model.node_name.clone(),
      kind_key: model.kind_key.clone(),
      created_at: model
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().naive_utc()),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}
