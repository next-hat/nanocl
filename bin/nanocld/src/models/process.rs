use std::sync::Arc;

use diesel::prelude::*;
use ntex::rt::JoinHandle;

use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocl_stubs::{
  generic::{GenericFilter, GenericClause},
  process::{Process, ProcessKind, ProcessPartial},
};

use crate::{utils, gen_where4string, schema::processes};

use super::{Pool, Repository};

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
  pub node_key: String,
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
      created_at: model
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().naive_utc()),
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
    log::trace!("ProcesssDb::find_one: {filter:?}");
    let query = gen_query(filter, false);
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
    log::trace!("ProcesssDb::find: {filter:?}");
    let query = gen_query(filter, true);
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

fn gen_query(
  filter: &GenericFilter,
  is_multiple: bool,
) -> processes::BoxedQuery<'static, diesel::pg::Pg> {
  let r#where = filter.r#where.to_owned().unwrap_or_default();
  let mut query = processes::dsl::processes.into_boxed();
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
  if is_multiple {
    query = query.order(processes::dsl::created_at.desc());
    let limit = filter.limit.unwrap_or(100);
    query = query.limit(limit as i64);
    if let Some(offset) = filter.offset {
      query = query.offset(offset as i64);
    }
  }
  query
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
