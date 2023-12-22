use diesel::prelude::*;

use nanocl_error::io::IoResult;
use nanocl_stubs::{
  generic::GenericFilter,
  job::{Job, JobPartial},
};

use crate::{
  models::{JobDb, JobUpdateDb},
  schema::jobs,
};

use super::generic::*;

impl RepositoryBase for JobDb {}

impl RepositoryCreate for JobDb {}

impl RepositoryUpdate for JobDb {
  type UpdateItem = JobUpdateDb;
}

impl RepositoryDelByPk for JobDb {}

impl RepositoryRead for JobDb {
  type Output = JobDb;
  type Query = jobs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let mut query = jobs::dsl::jobs.into_boxed();
    if is_multiple {
      query = query.order(jobs::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}

impl JobDb {
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
