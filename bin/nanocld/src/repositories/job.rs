use std::collections::HashMap;

use diesel::prelude::*;

use futures::stream::FuturesOrdered;
use futures_util::StreamExt;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::{
  generic::GenericFilter,
  job::{Job, JobPartial, JobSummary},
  process::ProcessKind,
  system::{ObjPsHealthStatusKind, ObjPsStatusKind, ObjPsStatusPartial},
};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    ColumnType, JobDb, JobUpdateDb, ObjPsStatusDb, Pool, ProcessDb,
    ResourceIncarnationDb, SpecDb, SystemState,
  },
  schema::{jobs, object_process_statuses, resource_incarnations, specs},
  utils,
};

use super::generic::*;

impl RepositoryBase for JobDb {
  fn get_columns<'a>()
  -> std::collections::HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::from([
      ("key", (ColumnType::Text, "jobs.key")),
      ("data", (ColumnType::Json, "specs.data")),
      ("metadata", (ColumnType::Json, "specs.metadata")),
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
  type Output = (JobDb, SpecDb, ObjPsStatusDb);

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
      .inner_join(specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    log::debug!("job filter: {:?}", filter);
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      log::debug!("job default order by created_at.desc()");
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
      .inner_join(specs::table)
      .inner_join(crate::schema::object_process_statuses::table)
      .into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl RepositoryReadByTransform for JobDb {
  type NewOutput = Job;

  fn transform(
    item: (JobDb, SpecDb, ObjPsStatusDb),
  ) -> IoResult<Self::NewOutput> {
    let (job_db, spec, status) = item;
    let item = job_db.try_to_spec(&spec, &status)?;
    Ok(item)
  }
}

impl JobDb {
  pub fn validate_partial(p: &JobPartial) -> IoResult<()> {
    utils::key::validate_kind_key(&ProcessKind::Job, &p.name)?;
    p.validate_schedule()
      .map_err(|err| IoError::invalid_input("Job schedule", &err))?;
    for container in &p.containers {
      if let Some(policy) = container
        .host_config
        .as_ref()
        .and_then(|host| host.restart_policy.as_ref())
      {
        let name = policy
          .name
          .as_ref()
          .map(ToString::to_string)
          .unwrap_or_default();
        if !matches!(name.as_str(), "" | "no")
          || policy.maximum_retry_count.is_some_and(|count| count != 0)
        {
          return Err(IoError::invalid_input(
            "Job restart policy",
            "Job containers must disable Docker automatic restart; each execution is a separate run",
          ));
        }
      }
    }
    Ok(())
  }

  pub fn try_to_spec(
    &self,
    spec: &SpecDb,
    status: &ObjPsStatusDb,
  ) -> IoResult<Job> {
    Ok(Job {
      created_at: self.created_at,
      updated_at: self.updated_at,
      status: status.clone().try_into()?,
      incarnation_key: self.incarnation_key,
      spec_key: self.spec_key,
      current_run_key: self.current_run_key,
      spec: spec.try_to_job_spec()?,
    })
  }

  pub(crate) async fn create_with_spec(
    p: &JobPartial,
    version: &str,
    pool: &Pool,
  ) -> IoResult<Job> {
    use super::operation::{run_query_owned, transaction};
    Self::validate_partial(p)?;
    let now = chrono::Utc::now().naive_utc();
    let identity = ResourceIncarnationDb {
      key: uuid::Uuid::new_v4(),
      resource_kind: "Job".into(),
      resource_key: p.name.clone(),
      created_at: now,
      deleted_at: None,
      current_operation_key: None,
    };
    let spec = SpecDb {
      key: uuid::Uuid::new_v4(),
      created_at: now,
      kind_name: "Job".into(),
      kind_key: p.name.clone(),
      version: version.to_owned(),
      data: serde_json::to_value(p)?,
      metadata: p.metadata.clone(),
      incarnation_key: Some(identity.key),
    };
    let status = ObjPsStatusDb::from(ObjPsStatusPartial {
      key: p.name.clone(),
      wanted: ObjPsStatusKind::Create,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Create,
      prev_actual: ObjPsStatusKind::Create,
      health: ObjPsHealthStatusKind::Unknown,
    });
    let job = JobDb {
      key: p.name.clone(),
      created_at: now,
      updated_at: now,
      status_key: p.name.clone(),
      spec_key: spec.key,
      incarnation_key: identity.key,
      current_run_key: None,
    };
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        diesel::insert_into(resource_incarnations::table)
          .values(&identity)
          .execute(conn)?;
        diesel::insert_into(specs::table)
          .values(&spec)
          .execute(conn)?;
        diesel::insert_into(object_process_statuses::table)
          .values(&status)
          .execute(conn)?;
        diesel::insert_into(jobs::table)
          .values(&job)
          .execute(conn)?;
        Ok(job.try_to_spec(&spec, &status)?)
      })
    })
    .await
  }

  /// Process inventory is scoped by the current run's immutable container IDs.
  pub(crate) async fn current_processes(
    job: &Job,
    pool: &Pool,
  ) -> IoResult<Vec<nanocl_stubs::process::Process>> {
    use super::operation::run_query_owned;
    let Some(run) = job.current_run_key else {
      return Ok(Vec::new());
    };
    let name = job.spec.name.clone();
    run_query_owned(pool, (), move |conn| {
      use crate::schema::{job_run_steps, processes};
      let ids = job_run_steps::table
        .filter(job_run_steps::run_key.eq(run))
        .select(job_run_steps::container_id)
        .load::<Option<String>>(conn)
        .map_err(crate::models::OperationTransactionError::from)?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
      processes::table
        .filter(processes::key.eq_any(ids))
        .filter(processes::kind.eq("job"))
        .filter(processes::kind_key.eq(&name))
        .order(processes::created_at.asc())
        .load::<ProcessDb>(conn)
        .map_err(crate::models::OperationTransactionError::from)?
        .into_iter()
        .map(TryInto::try_into)
        .collect()
    })
    .await
  }

  /// Counts describe durable observed steps, never Docker's default exit code.
  pub(crate) async fn current_counts(
    job: &Job,
    pool: &Pool,
  ) -> IoResult<(usize, usize, usize, usize)> {
    let Some(run) = job.current_run_key else {
      return Ok((0, 0, 0, 0));
    };
    let steps = crate::models::JobRunDb::steps(run, pool).await?;
    let failed = steps.iter().filter(|step| step.state == "Failed").count();
    let succeeded = steps
      .iter()
      .filter(|step| step.state == "Succeeded")
      .count();
    let running = steps.iter().filter(|step| step.state == "Running").count();
    Ok((steps.len(), failed, succeeded, running))
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
        let (
          instance_total,
          instance_failed,
          instance_success,
          instance_running,
        ) = Self::current_counts(job, &state.inner.pool).await?;
        Ok::<_, HttpError>(JobSummary {
          created_at: job.created_at,
          updated_at: job.updated_at,
          status: job.status.clone(),
          incarnation_key: job.incarnation_key,
          spec_key: job.spec_key,
          current_run_key: job.current_run_key,
          instance_total,
          instance_success,
          instance_running,
          instance_failed,
          spec: job.spec.clone(),
        })
      })
      .collect::<FuturesOrdered<_>>()
      .collect::<Vec<HttpResult<_>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
    Ok(job_summaries)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn job_creation_rejects_automatic_container_retries() {
    use bollard_next::models::{
      ContainerCreateBody, HostConfig, RestartPolicy, RestartPolicyNameEnum,
    };
    for name in [
      RestartPolicyNameEnum::ALWAYS,
      RestartPolicyNameEnum::ON_FAILURE,
      RestartPolicyNameEnum::UNLESS_STOPPED,
    ] {
      let partial = JobPartial {
        name: "retry-policy-test".into(),
        containers: vec![ContainerCreateBody {
          host_config: Some(HostConfig {
            restart_policy: Some(RestartPolicy {
              name: Some(name),
              maximum_retry_count: None,
            }),
            ..Default::default()
          }),
          ..Default::default()
        }],
        ..Default::default()
      };
      assert!(JobDb::validate_partial(&partial).is_err());
    }
    let partial = JobPartial {
      name: "no-retry-policy-test".into(),
      containers: vec![ContainerCreateBody {
        host_config: Some(HostConfig {
          restart_policy: Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::NO),
            maximum_retry_count: Some(0),
          }),
          ..Default::default()
        }),
        ..Default::default()
      }],
      ..Default::default()
    };
    assert!(JobDb::validate_partial(&partial).is_ok());
  }

  #[test]
  fn job_creation_rejects_traversal_before_persistence() {
    for name in ["../../poc_escape", "../../store/certs", "/tmp/poc"] {
      let partial = JobPartial {
        name: name.to_owned(),
        ..Default::default()
      };
      assert!(JobDb::validate_partial(&partial).is_err());
    }
    let partial = JobPartial {
      name: "backup.daily".to_owned(),
      ..Default::default()
    };
    assert!(JobDb::validate_partial(&partial).is_ok());
  }
}
