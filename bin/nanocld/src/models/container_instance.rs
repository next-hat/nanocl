use serde::Deserialize;

use nanocl_error::io::{IoError, FromIo};

use bollard_next::service::ContainerInspectResponse;

use crate::schema::container_instances;

/// ## ContainerInstanceDb
///
/// This structure represent a job to run.
/// It will create and run a list of containers.
///
#[derive(Clone, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = container_instances)]
pub struct ContainerInstanceDb {
  /// The key of the job generated with the name
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// Last time the instance was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the container instance
  pub name: String,
  /// Kind of the container instance (job, vm, cargo)
  pub kind: String,
  /// The data of the container instance a ContainerInspect
  pub data: serde_json::Value,
  /// Id of the node where the container is running
  pub node_id: String,
  /// Id of the related kind
  pub kind_id: String,
}

/// ## ContainerInstancePartial
///
/// This structure represent a partial container instance.
/// It will create a new container instance.
///
#[derive(Debug, Clone)]
pub struct ContainerInstancePartial {
  pub key: String,
  pub name: String,
  pub kind: String,
  pub data: serde_json::Value,
  pub node_id: String,
  pub kind_id: String,
}

impl From<ContainerInstancePartial> for ContainerInstanceDb {
  fn from(model: ContainerInstancePartial) -> Self {
    Self {
      key: model.key,
      name: model.name,
      kind: model.kind,
      data: model.data,
      node_id: model.node_id,
      kind_id: model.kind_id,
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerInstance {
  pub key: String,
  pub name: String,
  pub kind: String,
  pub node_id: String,
  pub kind_id: String,
  pub data: ContainerInspectResponse,
}

impl TryFrom<ContainerInstanceDb> for ContainerInstance {
  type Error = IoError;

  fn try_from(model: ContainerInstanceDb) -> Result<Self, Self::Error> {
    Ok(Self {
      key: model.key,
      name: model.name,
      kind: model.kind,
      data: serde_json::from_value(model.data)
        .map_err(|err| err.map_err_context(|| "Container instance"))?,
      node_id: model.node_id,
      kind_id: model.kind_id,
    })
  }
}

/// ## ContainerInstanceUpdateDb
///
/// This structure represent the update of a container instance.
/// It will update the container instance with the new data.
///
#[derive(Clone, AsChangeset)]
#[diesel(table_name = container_instances)]
pub struct ContainerInstanceUpdateDb {
  /// Last time the instance was updated
  pub updated_at: Option<chrono::NaiveDateTime>,
  // The updated at data
  pub data: Option<serde_json::Value>,
}
