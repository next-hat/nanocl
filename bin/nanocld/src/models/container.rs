use std::{sync::Arc, collections::HashMap};

use serde::Deserialize;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoError, FromIo, IoResult};

use bollard_next::service::ContainerInspectResponse;
use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{schema::containers, gen_where4string, utils};

use super::{Pool, Repository};

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

/// Represents a container instance
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container {
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
  /// Id of the node where the container is running
  pub node_id: String,
  /// Id of the related kind
  pub kind_id: String,
  /// The data of the container instance a ContainerInspect
  pub data: ContainerInspectResponse,
}

/// Used to update a container instance
#[derive(Clone, AsChangeset)]
#[diesel(table_name = containers)]
pub struct ContainerUpdateDb {
  /// Last time the instance was updated
  pub updated_at: Option<chrono::NaiveDateTime>,
  // The updated at data
  pub data: Option<serde_json::Value>,
}

impl TryFrom<ContainerDb> for Container {
  type Error = IoError;

  fn try_from(model: ContainerDb) -> Result<Self, Self::Error> {
    Ok(Self {
      key: model.key,
      created_at: model.created_at,
      updated_at: model.updated_at,
      name: model.name,
      kind: model.kind,
      data: serde_json::from_value(model.data)
        .map_err(|err| err.map_err_context(|| "Container instance"))?,
      node_id: model.node_id,
      kind_id: model.kind_id,
    })
  }
}

impl std::convert::From<&ContainerPartial> for ContainerDb {
  fn from(model: &ContainerPartial) -> Self {
    Self {
      key: model.key.clone(),
      name: model.name.clone(),
      kind: model.kind.clone(),
      data: model.data.clone(),
      node_id: model.node_id.clone(),
      kind_id: model.kind_id.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}

impl Repository for ContainerDb {
  type Table = containers::table;
  type Item = Container;
  type UpdateItem = ContainerUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    unimplemented!()
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    let mut query = containers::dsl::containers
      .into_boxed()
      .order(containers::dsl::created_at.desc());
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    if let Some(value) = r#where.get("Key") {
      gen_where4string!(query, containers::dsl::key, value);
    }
    if let Some(value) = r#where.get("Name") {
      gen_where4string!(query, containers::dsl::name, value);
    }
    if let Some(value) = r#where.get("Kind") {
      gen_where4string!(query, containers::dsl::kind, value);
    }
    if let Some(value) = r#where.get("NodeId") {
      gen_where4string!(query, containers::dsl::node_id, value);
    }
    if let Some(value) = r#where.get("KindId") {
      gen_where4string!(query, containers::dsl::kind_id, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items: Vec<Self> = query
        .load(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?;
      let items = items
        .into_iter()
        .map(|item| {
          let item = Self::Item::try_from(item).map_err(|_| {
            IoError::invalid_data(std::any::type_name::<Self>(), "try_from")
          })?;
          Ok::<_, IoError>(item)
        })
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, nanocl_error::io::IoError>(items)
    })
  }
}

impl ContainerDb {
  pub(crate) async fn find_by_kind_id(
    kind_id: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Container>> {
    let mut r#where = HashMap::new();
    r#where.insert("KindId".to_owned(), GenericClause::Eq(kind_id.to_owned()));
    let filter = GenericFilter {
      r#where: Some(r#where),
    };
    ContainerDb::find(&filter, pool).await?
  }
}
