use std::sync::Arc;

use diesel::prelude::*;
use tokio::task::JoinHandle;

use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocl_stubs::{
  job::{Job, JobPartial},
  generic::GenericFilter,
};

use crate::{schema::jobs, utils};

use super::{generic::FromSpec, Repository, Pool};

/// This structure represent a job to run.
/// It will create and run a list of containers.
#[derive(Clone, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = jobs)]
pub struct JobDb {
  /// The key of the job generated with the name
  pub key: String,
  /// The created at date
  pub created_at: chrono::NaiveDateTime,
  /// The updated at data
  pub updated_at: chrono::NaiveDateTime,
  /// The spec
  pub data: serde_json::Value,
  /// The metadata
  pub metadata: Option<serde_json::Value>,
}

/// This structure represent the update of a job.
/// It will update the job with the new data.
#[derive(Clone, AsChangeset)]
#[diesel(table_name = jobs)]
pub struct JobUpdateDb {
  pub updated_at: Option<chrono::NaiveDateTime>,
}

impl Repository for JobDb {
  type Table = jobs::table;
  type Item = Job;
  type UpdateItem = JobUpdateDb;

  fn find_one(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Self::Item>> {
    let mut query = jobs::dsl::jobs
      .order(jobs::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    // if let Some(value) = r#where.get("Key") {
    //   gen_where4string!(query, stream_metrics::dsl::key, value);
    // }
    // if let Some(value) = r#where.get("Name") {
    //   gen_where4string!(query, stream_metrics::dsl::name, value);
    // }
    // if let Some(value) = r#where.get("Kind") {
    //   gen_where4string!(query, stream_metrics::dsl::kind, value);
    // }
    // if let Some(value) = r#where.get("NodeId") {
    //   gen_where4string!(query, stream_metrics::dsl::node_id, value);
    // }
    // if let Some(value) = r#where.get("KindId") {
    //   gen_where4string!(query, stream_metrics::dsl::kind_id, value);
    // }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_result::<Self>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?
        .try_to_spec()?;
      Ok::<_, IoError>(item)
    })
  }

  fn find(
    filter: &GenericFilter,
    pool: &Pool,
  ) -> JoinHandle<IoResult<Vec<Self::Item>>> {
    let mut query = jobs::dsl::jobs
      .order(jobs::dsl::created_at.desc())
      .into_boxed();
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    // if let Some(value) = r#where.get("Key") {
    //   gen_where4string!(query, stream_metrics::dsl::key, value);
    // }
    // if let Some(value) = r#where.get("Name") {
    //   gen_where4string!(query, stream_metrics::dsl::name, value);
    // }
    // if let Some(value) = r#where.get("Kind") {
    //   gen_where4string!(query, stream_metrics::dsl::kind, value);
    // }
    // if let Some(value) = r#where.get("NodeId") {
    //   gen_where4string!(query, stream_metrics::dsl::node_id, value);
    // }
    // if let Some(value) = r#where.get("KindId") {
    //   gen_where4string!(query, stream_metrics::dsl::kind_id, value);
    // }
    let pool = Arc::clone(pool);
    ntex::rt::spawn_blocking(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = query
        .get_results::<Self>(&mut conn)
        .map_err(|err| err.map_err_context(std::any::type_name::<Self>))?
        .into_iter()
        .map(|item| item.try_to_spec())
        .collect::<IoResult<Vec<_>>>()?;
      Ok::<_, IoError>(item)
    })
  }
}

impl FromSpec for JobDb {
  type Spec = Job;
  type SpecPartial = JobPartial;

  fn try_from_spec_partial(
    id: &str,
    _version: &str,
    p: &Self::SpecPartial,
  ) -> IoResult<Self> {
    let data = JobDb::try_to_data(p)?;
    Ok(JobDb {
      key: id.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      data,
      metadata: p.metadata.clone(),
    })
  }

  fn get_data(&self) -> &serde_json::Value {
    &self.data
  }

  fn to_spec(&self, p: &Self::SpecPartial) -> Self::Spec {
    Job {
      name: self.key.clone(),
      created_at: self.created_at,
      updated_at: self.updated_at,
      metadata: self.metadata.clone(),
      secrets: p.secrets.clone(),
      schedule: p.schedule.clone(),
      ttl: p.ttl,
      containers: p.containers.clone(),
    }
  }
}
