use std::sync::Arc;

use serde::Deserialize;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoError, IoResult, FromIo};

use bollard_next::service::ContainerInspectResponse;
use nanocl_stubs::generic::{GenericFilter, GenericClause};

use crate::{utils, gen_where4string};
use crate::schema::processes;

use super::{Pool, Repository};

/// Represents a process (job, cargo, vm) in the database
#[derive(Clone, Queryable, Identifiable, Insertable)]
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
  pub node_key: String,
  /// Id of the related kind
  pub kind_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub enum ProcessKind {
  Vm,
  Job,
  Cargo,
}

impl TryFrom<String> for ProcessKind {
  type Error = IoError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    match value.as_ref() {
      "vm" => Ok(Self::Vm),
      "job" => Ok(Self::Job),
      "cargo" => Ok(Self::Cargo),
      _ => Err(IoError::invalid_input(
        "ProcessKind",
        &format!("Invalid process kind: {value}"),
      )),
    }
  }
}

impl ToString for ProcessKind {
  fn to_string(&self) -> String {
    match self {
      Self::Vm => "vm",
      Self::Job => "job",
      Self::Cargo => "cargo",
    }
    .to_owned()
  }
}

/// Used to create a new process
#[derive(Debug, Clone)]
pub struct ProcessPartial {
  /// The key of the process
  pub key: String,
  /// Name of the process
  pub name: String,
  /// Kind of the process (Job, Vm, Cargo)
  pub kind: ProcessKind,
  /// The data of the process a ContainerInspect
  pub data: serde_json::Value,
  /// Key of the node where the container is running
  pub node_key: String,
  /// Key of the related kind
  pub kind_key: String,
}

/// Represents a process
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Process {
  /// The key of the process
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// Last time the instance was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Name of the process
  pub name: String,
  /// Kind of the process (Job, Vm, Cargo)
  pub kind: ProcessKind,
  /// Key of the node where the container is running
  pub node_key: String,
  /// Key of the related kind
  pub kind_key: String,
  /// The data of the process a ContainerInspect
  pub data: ContainerInspectResponse,
}

/// Used to update a process
#[derive(Clone, AsChangeset)]
#[diesel(table_name = processes)]
pub struct ProcessUpdateDb {
  /// Last time the instance was updated
  pub updated_at: Option<chrono::NaiveDateTime>,
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
      node_key: model.node_key,
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
      node_key: model.node_key.clone(),
      kind_key: model.kind_key.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
    }
  }
}

impl Repository for ProcessDb {
  type Table = processes::table;
  type Item = Process;
  type UpdateItem = ProcessUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    log::debug!("ContainerDb::find_one filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = processes::dsl::processes
      .order(processes::dsl::created_at.desc())
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::dsl::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::dsl::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::dsl::kind_key, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(Self::map_err_context)?;
      let item = Self::Item::try_from(item)?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    log::debug!("ContainerDb::find filter: {filter:?}");
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = processes::dsl::processes
      .order(processes::dsl::created_at.desc())
      .into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4string!(query, processes::dsl::key, value);
    }
    if let Some(value) = r#where.get("name") {
      gen_where4string!(query, processes::dsl::name, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, processes::dsl::kind, value);
    }
    if let Some(value) = r#where.get("node_key") {
      gen_where4string!(query, processes::dsl::node_key, value);
    }
    if let Some(value) = r#where.get("kind_key") {
      gen_where4string!(query, processes::dsl::kind_key, value);
    }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let items = query
        .get_results::<Self>(&mut conn)
        .map_err(Self::map_err_context)?
        .into_iter()
        .map(Self::Item::try_from)
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(items)
    })
  }
}

impl ProcessDb {
  pub(crate) async fn find_by_kind_key(
    kind_key: &str,
    pool: &Pool,
  ) -> IoResult<Vec<Process>> {
    let filter = GenericFilter::new()
      .r#where("kind_key", GenericClause::Eq(kind_key.to_owned()));
    ProcessDb::find(&filter, pool).await?
  }
}
