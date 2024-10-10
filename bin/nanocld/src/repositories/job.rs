use std::collections::HashMap;

use diesel::prelude::*;

use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoResult,
};
use nanocl_stubs::{
  generic::GenericFilter,
  job::{Job, JobPartial, JobSummary},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    ColumnType, JobDb, JobUpdateDb, ObjPsStatusDb, Pool, ProcessDb, SystemState,
  },
  schema::jobs,
  utils,
};

use super::generic::*;

impl RepositoryBase for JobDb {
  fn get_columns<'a>(
  ) -> std::collections::HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "jobs.key")),
      ("data", (ColumnType::Json, "jobs.data")),
      ("metadata", (ColumnType::Json, "jobs.metadata")),
      ("created_at", (ColumnType::Timestamptz, "jobs.created_at")),
      ("updated_at", (ColumnType::Timestamptz, "jobs.updated_at")),
      (
        "status.wanted",
        (ColumnType::Text, "object_process_statuses.wanted"),
      ),
      (
        "status.actual",
        (ColumnType::Text, "object_process_statuses.actual"),
      ),
    ])
  }
}

impl RepositoryCreate for JobDb {}

impl RepositoryUpdate for JobDb {
  type UpdateItem = JobUpdateDb;
}

impl RepositoryDelByPk for JobDb {}

impl RepositoryReadBy for JobDb {
  type Output = (JobDb, ObjPsStatusDb);

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
    let mut query = jobs::table
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(jobs::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for JobDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::methods::LoadQuery<'static, diesel::PgConnection, i64>
  {
    let mut query = jobs::table
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for JobDb {
  type NewOutput = Job;

  fn transform(item: (JobDb, ObjPsStatusDb)) -> IoResult<Self::NewOutput> {
    let (job_db, status) = item;
    let item = job_db.try_to_spec(&status)?;
    Ok(item)
  }
}

impl JobDb {
  pub async fn clear_by_pk(pk: &str, pool: &Pool) -> IoResult<()> {
    JobDb::del_by_pk(pk, pool).await?;
    ObjPsStatusDb::del_by_pk(pk, pool).await?;
    Ok(())
  }

  pub fn try_from_partial(p: &JobPartial) -> IoResult<Self> {
    let data = serde_json::to_value(p)?;
    Ok(JobDb {
      key: p.name.clone(),
      status_key: p.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      metadata: p.metadata.clone(),
      data,
    })
  }

  pub fn try_to_spec(&self, status: &ObjPsStatusDb) -> IoResult<Job> {
    let p = serde_json::from_value::<JobPartial>(self.data.clone())?;
    Ok(Job {
      name: self.key.clone(),
      created_at: self.created_at,
      updated_at: self.updated_at,
      metadata: self.metadata.clone(),
      secrets: p.secrets.clone(),
      schedule: p.schedule.clone(),
      ttl: p.ttl,
      status: status.clone().try_into()?,
      containers: p.containers.clone(),
      image_pull_secret: p.image_pull_secret.clone(),
      image_pull_policy: p.image_pull_policy.clone(),
    })
  }

  /// List all jobs
  pub async fn list(
    filter: &GenericFilter,
    state: &SystemState,
  ) -> HttpResult<Vec<JobSummary>> {
    let jobs = JobDb::transform_read_by(filter, &state.inner.pool).await?;
    let job_summaries = jobs
      .iter()
      .map(|job| async {
        let instances =
          ProcessDb::read_by_kind_key(&job.name, &state.inner.pool).await?;
        let (
          instance_total,
          instance_failed,
          instance_success,
          instance_running,
        ) = utils::container::generic::count_status(&instances);
        Ok::<_, HttpError>(JobSummary {
          instance_total,
          instance_success,
          instance_running,
          instance_failed,
          spec: job.clone(),
        })
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<_>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
    Ok(job_summaries)
  }
}
