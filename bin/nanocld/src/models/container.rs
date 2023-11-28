use serde::Deserialize;

use nanocl_error::io::{IoError, FromIo};

use bollard_next::service::ContainerInspectResponse;

use crate::schema::containers;

/// Represents a container instance in the database
#[derive(Clone, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = containers)]
pub struct ContainerDb {
  /// The key of the container instance
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

/// Used to create a new container instance
#[derive(Debug, Clone)]
pub struct ContainerPartial {
  /// The key of the container instance
  pub key: String,
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

impl From<ContainerPartial> for ContainerDb {
  fn from(model: ContainerPartial) -> Self {
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

/// Represents a container instance
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
  /// The key of the container instance
  pub key: String,
  /// Name of the container instance
  pub name: String,
  /// Kind of the container instance (job, vm, cargo)
  pub kind: String,
  /// Id of the node where the container is running
  pub node_id: String,
  /// Id of the related kind
  pub kind_id: String,
  /// The data of the container instance a ContainerInspect
  pub data: ContainerInspectResponse,
}

impl TryFrom<ContainerDb> for Container {
  type Error = IoError;

  fn try_from(model: ContainerDb) -> Result<Self, Self::Error> {
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

/// Used to update a container instance
#[derive(Clone, AsChangeset)]
#[diesel(table_name = containers)]
pub struct ContainerInstanceUpdateDb {
  /// Last time the instance was updated
  pub updated_at: Option<chrono::NaiveDateTime>,
  // The updated at data
  pub data: Option<serde_json::Value>,
}
