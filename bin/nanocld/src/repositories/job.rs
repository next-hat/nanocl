use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::{
  generic::GenericFilter,
  job::{Job, JobPartial},
};

use crate::{
  gen_multiple, gen_where4json, gen_where4string,
  models::{JobDb, JobUpdateDb, Pool, ObjPsStatusDb},
  schema::jobs,
};

use super::generic::*;

impl RepositoryBase for JobDb {}

impl RepositoryCreate for JobDb {}

impl RepositoryUpdate for JobDb {
  type UpdateItem = JobUpdateDb;
}

impl RepositoryDelByPk for JobDb {}

impl RepositoryReadBy for JobDb {
  type Output = JobDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  > {
    let r#where = filter.r#where.clone().unwrap_or_default();
    let mut query = jobs::table.into_boxed();
    if let Some(key) = r#where.get("key") {
      gen_where4string!(query, jobs::key, key);
    }
    if let Some(data) = r#where.get("data") {
      gen_where4json!(query, jobs::data, data);
    }
    if let Some(metadata) = r#where.get("metadata") {
      gen_where4json!(query, jobs::metadata, metadata);
    }
    if is_multiple {
      gen_multiple!(query, jobs::created_at, filter);
    }
    query
  }
}

impl JobDb {
  pub async fn clear(pk: &str, pool: &Pool) -> IoResult<()> {
    JobDb::del_by_pk(pk, pool).await?;
    ObjPsStatusDb::del_by_pk(pk, pool).await?;
    Ok(())
  }

  pub fn to_spec(&self, p: &JobPartial) -> Job {
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

  pub fn try_from_partial(p: &JobPartial) -> IoResult<Self> {
    let data = serde_json::to_value(p)?;
    Ok(JobDb {
      key: p.name.clone(),
      status_key: p.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      metadata: Default::default(),
      data,
    })
  }

  pub fn try_to_spec(&self) -> IoResult<Job> {
    let p = serde_json::from_value::<JobPartial>(self.data.clone())?;
    Ok(Job {
      name: self.key.clone(),
      created_at: self.created_at,
      updated_at: self.updated_at,
      metadata: self.metadata.clone(),
      secrets: p.secrets.clone(),
      schedule: p.schedule.clone(),
      ttl: p.ttl,
      containers: p.containers.clone(),
    })
  }
}
