use diesel::prelude::*;
use nanocl_error::io::IoResult;
use nanocl_stubs::job::Job;

use super::operation::{
  self, TransactionResult, conflict, run_query_owned, transaction,
};
use crate::{
  models::{
    AcceptedJobControl, AcceptedJobRun, DistributedMutexDb, JobDb, JobRunDb,
    JobRunStepDb, ObjPsStatusDb, OperationDb, Pool, SpecDb,
  },
  schema::{
    distributed_mutexes, job_run_steps, job_runs, jobs,
    object_process_statuses, resource_incarnations, specs,
  },
};

fn lock_job(
  conn: &mut PgConnection,
  expected: &Job,
  operation: &OperationDb,
) -> TransactionResult<(JobDb, SpecDb, ObjPsStatusDb)> {
  let identity = operation::lock_incarnation(conn, operation.incarnation_key)?;
  let job = jobs::table
    .find(&expected.spec.name)
    .for_update()
    .first::<JobDb>(conn)?;
  if identity.deleted_at.is_some()
    || identity.resource_kind != "Job"
    || identity.resource_key != job.key
    || job.incarnation_key != identity.key
    || job.incarnation_key != expected.incarnation_key
    || job.spec_key != expected.spec_key
    || job.current_run_key != expected.current_run_key
    || operation.resource_kind != "Job"
    || operation.resource_key != job.key
    || operation.spec_key != job.spec_key
  {
    return Err(conflict(
      "Job incarnation, revision, or current run changed during acceptance",
    ));
  }
  let mutex = distributed_mutexes::table
    .find(operation.key)
    .for_update()
    .first::<DistributedMutexDb>(conn)?;
  if mutex.resource_kind != "Job"
    || mutex.resource_key != job.key
    || mutex.node_key != operation.node_name
  {
    return Err(conflict("Job operation does not own its exact mutex"));
  }
  let spec = specs::table.find(job.spec_key).first::<SpecDb>(conn)?;
  let status = object_process_statuses::table
    .find(&job.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  operation::accept_on_conn(conn, &identity, operation)?;
  Ok((job, spec, status))
}

pub(crate) fn write_status(
  conn: &mut PgConnection,
  job: &JobDb,
  wanted: &str,
  actual: &str,
) -> TransactionResult<ObjPsStatusDb> {
  let previous = object_process_statuses::table
    .find(&job.status_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  Ok(
    diesel::update(object_process_statuses::table.find(&job.status_key))
      .set((
        object_process_statuses::wanted.eq(wanted),
        object_process_statuses::prev_wanted.eq(previous.wanted),
        object_process_statuses::actual.eq(actual),
        object_process_statuses::prev_actual.eq(previous.actual),
        object_process_statuses::health.eq("unknown"),
        object_process_statuses::updated_at.eq(chrono::Utc::now().naive_utc()),
      ))
      .get_result(conn)?,
  )
}

impl JobDb {
  pub(crate) async fn accept_run<G: Send + 'static>(
    expected: &Job,
    operation: OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<AcceptedJobRun> {
    let expected = expected.clone();
    let run_key = uuid::Uuid::new_v4();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        if operation.action != "start" {
          return Err(conflict("Run admission requires a start operation"));
        }
        let (mut job, spec, _) = lock_job(conn, &expected, &operation)?;
        if job_runs::table
          .filter(job_runs::incarnation_key.eq(job.incarnation_key))
          .filter(job_runs::finished_at.is_null())
          .select(job_runs::key)
          .first::<uuid::Uuid>(conn)
          .optional()?
          .is_some()
        {
          return Err(conflict("Job already has an active run"));
        }
        let definition: nanocl_stubs::job::JobPartial =
          serde_json::from_value(spec.data.clone())
            .map_err(nanocl_error::io::IoError::from)?;
        let run = JobRunDb {
          key: run_key,
          job_name: job.key.clone(),
          incarnation_key: job.incarnation_key,
          spec_key: job.spec_key,
          operation_key: operation.key,
          accepted_at: operation.accepted_at,
          started_at: None,
          finished_at: None,
          state: "Pending".into(),
          reason: None,
          cancellation_requested: false,
        };
        diesel::insert_into(job_runs::table)
          .values(&run)
          .execute(conn)?;
        for index in 0..definition.containers.len() {
          let step_index = i32::try_from(index)
            .map_err(|_| conflict("Job has too many steps"))?;
          let step = JobRunStepDb {
            run_key,
            step_index,
            container_id: None,
            state: "Pending".into(),
            started_at: None,
            finished_at: None,
            exit_code: None,
            error: None,
          };
          diesel::insert_into(job_run_steps::table)
            .values(&step)
            .execute(conn)?;
        }
        job = diesel::update(jobs::table.find(&job.key))
          .set((
            jobs::current_run_key.eq(run_key),
            jobs::updated_at.eq(operation.accepted_at),
          ))
          .get_result(conn)?;
        let status = write_status(conn, &job, "start", "starting")?;
        Ok(AcceptedJobRun {
          job: job.try_to_spec(&spec, &status)?,
          operation: operation.clone(),
          run,
        })
      })
    })
    .await
  }

  /// Dispatch acknowledgement releases the control slot, not the run's owner.
  pub(crate) async fn complete_start<G: Send + 'static>(
    operation: &OperationDb,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    Self::complete_control(operation, "completed", None, keepalive, pool).await
  }

  pub(crate) async fn accept_control<G: Send + 'static>(
    expected: &Job,
    operation: OperationDb,
    expected_run: Option<uuid::Uuid>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<AcceptedJobControl> {
    let expected = expected.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        if !matches!(operation.action.as_str(), "stop" | "delete") {
          return Err(conflict("Unsupported Job control operation"));
        }
        let (job, spec, _) = lock_job(conn, &expected, &operation)?;
        if expected_run.is_some() && job.current_run_key != expected_run {
          return Err(conflict("Job current run changed"));
        }
        let mut run = job
          .current_run_key
          .map(|key| {
            job_runs::table
              .find(key)
              .for_update()
              .first::<JobRunDb>(conn)
          })
          .transpose()?;
        if let Some(active) =
          run.as_mut().filter(|run| run.finished_at.is_none())
        {
          *active = diesel::update(job_runs::table.find(active.key))
            .set((
              job_runs::cancellation_requested.eq(true),
              job_runs::state.eq("Stopping"),
            ))
            .get_result(conn)?;
        }
        let (wanted, actual) = if operation.action == "delete" {
          ("destroy", "destroying")
        } else {
          ("stop", "stopping")
        };
        let status = write_status(conn, &job, wanted, actual)?;
        Ok(AcceptedJobControl {
          job: job.try_to_spec(&spec, &status)?,
          operation: operation.clone(),
          run,
        })
      })
    })
    .await
  }

  pub(crate) async fn complete_control<G: Send + 'static>(
    operation: &OperationDb,
    outcome: &str,
    reason: Option<String>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    let outcome = outcome.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        if operation::lock_current(conn, &operation)?.is_none() {
          return Ok(false);
        }
        let diagnostic = reason
          .as_ref()
          .map(|reason| serde_json::json!({"Reason":reason}));
        if operation.action != "start"
          && let Some(job) = jobs::table
            .find(&operation.resource_key)
            .for_update()
            .first::<JobDb>(conn)
            .optional()?
          && job.incarnation_key == operation.incarnation_key
        {
          let actual = if outcome == "completed" {
            "stop"
          } else {
            "unknown"
          };
          let wanted = if operation.action == "delete" {
            "destroy"
          } else {
            "stop"
          };
          write_status(conn, &job, wanted, actual)?;
        }
        operation::complete_on_conn(
          conn,
          &operation,
          &outcome,
          diagnostic.as_ref(),
        )?;
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn delete_operation<G: Send + 'static>(
    operation: &OperationDb,
    expected_run: Option<uuid::Uuid>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let operation = operation.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(identity) = operation::lock_current(conn, &operation)? else {
          return Ok(false);
        };
        if operation.resource_kind != "Job"
          || operation.action != "delete"
          || identity.deleted_at.is_some()
        {
          return Ok(false);
        }
        let Some(job) = jobs::table
          .find(&operation.resource_key)
          .for_update()
          .first::<JobDb>(conn)
          .optional()?
        else {
          return Ok(false);
        };
        if job.incarnation_key != operation.incarnation_key
          || job.spec_key != operation.spec_key
          || (expected_run.is_some() && job.current_run_key != expected_run)
        {
          return Ok(false);
        }
        let active = job_runs::table
          .filter(job_runs::incarnation_key.eq(identity.key))
          .filter(job_runs::finished_at.is_null())
          .select(job_runs::key)
          .first::<uuid::Uuid>(conn)
          .optional()?;
        if active.is_some() {
          return Err(conflict("Job runtime is still active or unobserved"));
        }
        diesel::delete(jobs::table.find(&job.key)).execute(conn)?;
        diesel::delete(object_process_statuses::table.find(&job.status_key))
          .execute(conn)?;
        diesel::update(resource_incarnations::table.find(identity.key))
          .set(
            resource_incarnations::deleted_at
              .eq(chrono::Utc::now().naive_utc()),
          )
          .execute(conn)?;
        operation::complete_on_conn(conn, &operation, "completed", None)?;
        Ok(true)
      })
    })
    .await
  }
}
