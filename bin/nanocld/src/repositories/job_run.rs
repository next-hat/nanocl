use diesel::prelude::*;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::job::{
  JobHistoryQuery, JobRun, JobRunInspect, JobRunOperation, JobRunStep,
};

use super::{
  job_operation::write_status,
  operation::{
    self, TransactionResult, conflict, run_query_owned, transaction,
  },
};
use crate::{
  models::{
    JobDb, JobRunDb, JobRunStepDb, OperationDb, OperationTransactionError,
    Pool, SpecDb,
  },
  schema::{
    job_run_steps, job_runs, jobs, object_process_statuses, operations, specs,
  },
};

fn same_run(left: &JobRunDb, right: &JobRunDb) -> bool {
  left.key == right.key
    && left.incarnation_key == right.incarnation_key
    && left.spec_key == right.spec_key
    && left.operation_key == right.operation_key
    && left.job_name == right.job_name
}

/// Always lock the retained incarnation first, shared with control acceptance.
/// History remains writable after the live row is removed, but never terminal
/// history or the live status of another run.
fn lock_run(
  conn: &mut PgConnection,
  expected: &JobRunDb,
) -> TransactionResult<Option<JobRunDb>> {
  let identity = operation::lock_incarnation(conn, expected.incarnation_key)?;
  if identity.resource_kind != "Job"
    || identity.resource_key != expected.job_name
  {
    return Ok(None);
  }
  let run = job_runs::table
    .find(expected.key)
    .for_update()
    .first::<JobRunDb>(conn)
    .optional()?;
  Ok(run.filter(|run| same_run(run, expected) && run.finished_at.is_none()))
}

fn current_job(
  conn: &mut PgConnection,
  run: &JobRunDb,
) -> TransactionResult<Option<JobDb>> {
  Ok(
    jobs::table
      .find(&run.job_name)
      .for_update()
      .first::<JobDb>(conn)
      .optional()?
      .filter(|job| {
        job.incarnation_key == run.incarnation_key
          && job.current_run_key == Some(run.key)
          && job.spec_key == run.spec_key
      }),
  )
}

fn executable(
  conn: &mut PgConnection,
  run: &JobRunDb,
) -> TransactionResult<bool> {
  Ok(
    !run.cancellation_requested
      && matches!(run.state.as_str(), "Pending" | "Running")
      && current_job(conn, run)?.is_some(),
  )
}

fn write_run_status(
  conn: &mut PgConnection,
  job: &JobDb,
  actual: &str,
) -> TransactionResult<()> {
  // A runtime observation cannot replace the desired stop/delete action.
  let wanted = object_process_statuses::table
    .find(&job.status_key)
    .select(object_process_statuses::wanted)
    .first::<String>(conn)?;
  write_status(conn, job, &wanted, actual)?;
  Ok(())
}

impl JobRunDb {
  pub(crate) fn to_public(&self) -> IoResult<JobRun> {
    Ok(JobRun {
      key: self.key,
      job_name: self.job_name.clone(),
      incarnation_key: self.incarnation_key,
      spec_key: self.spec_key,
      operation_key: self.operation_key,
      accepted_at: self.accepted_at,
      started_at: self.started_at,
      finished_at: self.finished_at,
      state: serde_json::from_value(serde_json::json!(self.state))?,
      reason: self.reason.clone(),
    })
  }

  pub(crate) async fn get(key: uuid::Uuid, pool: &Pool) -> IoResult<Self> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        job_runs::table
          .find(key)
          .first(conn)
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn active(pool: &Pool) -> IoResult<Vec<Self>> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        job_runs::table
          .filter(job_runs::finished_at.is_null())
          .order((job_runs::accepted_at.asc(), job_runs::key.asc()))
          .load(conn)
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn steps(
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<Vec<JobRunStepDb>> {
    run_query_owned(pool, (), move |conn| {
      Ok(
        job_run_steps::table
          .filter(job_run_steps::run_key.eq(key))
          .order(job_run_steps::step_index.asc())
          .load(conn)
          .map_err(OperationTransactionError::from)?,
      )
    })
    .await
  }

  pub(crate) async fn can_execute<G: Send + 'static>(
    run: &Self,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        executable(conn, &run)
      })
    })
    .await
  }

  /// Reserve the next step before its Docker create call. Later steps cannot be
  /// reserved until every predecessor has a confirmed zero exit.
  pub(crate) async fn begin_step<G: Send + 'static>(
    run: &Self,
    index: i32,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        if !executable(conn, &run)? {
          return Ok(false);
        }
        let previous = job_run_steps::table
          .filter(job_run_steps::run_key.eq(run.key))
          .filter(job_run_steps::step_index.lt(index))
          .load::<JobRunStepDb>(conn)?;
        if index < 0
          || previous.len() != index as usize
          || previous
            .iter()
            .any(|step| step.state != "Succeeded" || step.exit_code != Some(0))
        {
          return Ok(false);
        }
        let changed = diesel::update(
          job_run_steps::table
            .find((run.key, index))
            .filter(job_run_steps::state.eq("Pending")),
        )
        .set(job_run_steps::state.eq("Creating"))
        .execute(conn)?;
        Ok(changed == 1)
      })
    })
    .await
  }

  /// A completed Docker create must remain recorded even if stop was accepted
  /// while the request was in flight. This does not grant permission to start.
  pub(crate) async fn attach_container<G: Send + 'static>(
    run: &Self,
    index: i32,
    id: &str,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    let id = id.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        let step = job_run_steps::table
          .find((run.key, index))
          .for_update()
          .first::<JobRunStepDb>(conn)?;
        if step
          .container_id
          .as_ref()
          .is_some_and(|stored| stored != &id)
          || !matches!(step.state.as_str(), "Creating" | "Unknown")
        {
          return Ok(false);
        }
        diesel::update(job_run_steps::table.find((run.key, index)))
          .set(job_run_steps::container_id.eq(&id))
          .execute(conn)?;
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn update_step<G: Send + 'static>(
    run: &Self,
    index: i32,
    state: &str,
    exit_code: Option<i64>,
    error: Option<String>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    let state = state.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        let step = job_run_steps::table
          .find((run.key, index))
          .for_update()
          .first::<JobRunStepDb>(conn)?;
        if step.finished_at.is_some() {
          return Ok(false);
        }
        if !matches!(
          state.as_str(),
          "Running"
            | "Succeeded"
            | "Failed"
            | "Interrupted"
            | "Unknown"
            | "Skipped"
        ) {
          return Err(conflict("Invalid observed Job step state"));
        }
        if state == "Succeeded"
          && (exit_code != Some(0) || step.container_id.is_none())
        {
          return Err(conflict(
            "Step success requires a container and observed zero exit",
          ));
        }
        if state == "Running" && step.container_id.is_none() {
          return Err(conflict("Running step requires its exact container"));
        }
        if state == "Skipped"
          && (exit_code.is_some() || step.container_id.is_some())
        {
          return Err(conflict("An existing container cannot be skipped"));
        }
        let terminal = matches!(
          state.as_str(),
          "Succeeded" | "Failed" | "Interrupted" | "Skipped"
        );
        let now = chrono::Utc::now().naive_utc();
        let started = if state == "Running" {
          step.started_at.or(Some(now))
        } else {
          step.started_at
        };
        // Later runtime evidence can resolve uncertainty without erasing the
        // earlier create/start/wait error that explains this execution.
        let error = error.clone().or(step.error);
        diesel::update(job_run_steps::table.find((run.key, index)))
          .set((
            job_run_steps::state.eq(&state),
            job_run_steps::exit_code.eq(exit_code),
            job_run_steps::error.eq(&error),
            job_run_steps::started_at.eq(started),
            job_run_steps::finished_at.eq(terminal.then_some(now)),
          ))
          .execute(conn)?;
        if state == "Running" {
          diesel::update(job_runs::table.find(run.key))
            .set((
              job_runs::started_at.eq(run.started_at.or(Some(now))),
              job_runs::state.eq(if run.cancellation_requested {
                "Stopping"
              } else {
                "Running"
              }),
            ))
            .execute(conn)?;
          if !run.cancellation_requested
            && let Some(job) = current_job(conn, &run)?
          {
            write_run_status(conn, &job, "start")?;
          }
        }
        Ok(true)
      })
    })
    .await
  }

  pub(crate) async fn mark_unknown<G: Send + 'static>(
    run: &Self,
    reason: String,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        diesel::update(job_runs::table.find(run.key))
          .set((job_runs::state.eq("Unknown"), job_runs::reason.eq(&reason)))
          .execute(conn)?;
        if let Some(job) = current_job(conn, &run)? {
          write_run_status(conn, &job, "unknown")?;
        }
        Ok(true)
      })
    })
    .await
  }

  /// Terminal state is accepted only after every created container has a
  /// terminal observation. Unknown work deliberately retains the active slot.
  pub(crate) async fn finish<G: Send + 'static>(
    run: &Self,
    state: &str,
    reason: Option<String>,
    keepalive: G,
    pool: &Pool,
  ) -> IoResult<bool> {
    let run = run.clone();
    let state = state.to_owned();
    run_query_owned(pool, keepalive, move |conn| {
      transaction(conn, |conn| {
        let Some(run) = lock_run(conn, &run)? else {
          return Ok(false);
        };
        if !matches!(state.as_str(), "Succeeded" | "Failed" | "Interrupted") {
          return Err(conflict("Run outcome is not terminal"));
        }
        if state == "Succeeded" && run.cancellation_requested {
          return Ok(false);
        }
        let steps = job_run_steps::table
          .filter(job_run_steps::run_key.eq(run.key))
          .load::<JobRunStepDb>(conn)?;
        if state == "Succeeded"
          && steps
            .iter()
            .any(|step| step.state != "Succeeded" || step.exit_code != Some(0))
        {
          return Err(conflict("Run success requires every step to succeed"));
        }
        if steps.iter().any(|step| {
          step.finished_at.is_none()
            && (step.container_id.is_some()
              || matches!(
                step.state.as_str(),
                "Creating" | "Running" | "Unknown"
              ))
        }) {
          return Err(conflict("Run still has unobserved runtime work"));
        }
        let now = chrono::Utc::now().naive_utc();
        diesel::update(
          job_run_steps::table
            .filter(job_run_steps::run_key.eq(run.key))
            .filter(job_run_steps::finished_at.is_null())
            .filter(job_run_steps::container_id.is_null()),
        )
        .set((
          job_run_steps::state.eq("Skipped"),
          job_run_steps::finished_at.eq(now),
        ))
        .execute(conn)?;
        diesel::update(job_runs::table.find(run.key))
          .set((
            job_runs::state.eq(&state),
            job_runs::finished_at.eq(now),
            job_runs::reason.eq(&reason),
          ))
          .execute(conn)?;
        if let Some(job) = current_job(conn, &run)? {
          let actual = match state.as_str() {
            "Succeeded" => "finish",
            "Failed" => "fail",
            _ => "stop",
          };
          write_run_status(conn, &job, actual)?;
        }
        Ok(true)
      })
    })
    .await
  }
}

impl JobRunStepDb {
  fn to_public(&self) -> IoResult<JobRunStep> {
    Ok(JobRunStep {
      run_key: self.run_key,
      step_index: self.step_index,
      container_id: self.container_id.clone(),
      state: serde_json::from_value(serde_json::json!(self.state))?,
      started_at: self.started_at,
      finished_at: self.finished_at,
      exit_code: self.exit_code,
      error: self.error.clone(),
    })
  }
}

impl JobDb {
  pub(crate) async fn runs(
    name: &str,
    query: &JobHistoryQuery,
    pool: &Pool,
  ) -> IoResult<Vec<JobRun>> {
    let (limit, offset) = query
      .validate()
      .map_err(|error| IoError::invalid_input("Job history", &error))?;
    let name = name.to_owned();
    run_query_owned(pool, (), move |conn| {
      job_runs::table
        .filter(job_runs::job_name.eq(name))
        .order((job_runs::accepted_at.desc(), job_runs::key.desc()))
        .limit(limit)
        .offset(offset)
        .load::<JobRunDb>(conn)
        .map_err(OperationTransactionError::from)?
        .iter()
        .map(JobRunDb::to_public)
        .collect()
    })
    .await
  }

  pub(crate) async fn inspect_run(
    name: &str,
    key: uuid::Uuid,
    pool: &Pool,
  ) -> IoResult<JobRunInspect> {
    let name = name.to_owned();
    run_query_owned(pool, (), move |conn| {
      transaction(conn, |conn| {
        let run = job_runs::table
          .find(key)
          .filter(job_runs::job_name.eq(&name))
          .first::<JobRunDb>(conn)
          .optional()?
          .ok_or_else(|| {
            IoError::not_found(
              "Job run",
              "No retained run with this name and identity",
            )
          })?;
        let spec = specs::table.find(run.spec_key).first::<SpecDb>(conn)?;
        let op = operations::table
          .find(run.operation_key)
          .first::<OperationDb>(conn)?;
        let steps = job_run_steps::table
          .filter(job_run_steps::run_key.eq(run.key))
          .order(job_run_steps::step_index.asc())
          .load::<JobRunStepDb>(conn)?;
        Ok(JobRunInspect {
          run: run.to_public()?,
          spec: spec.try_to_job_spec()?,
          operation: JobRunOperation {
            key: op.key,
            incarnation_key: op.incarnation_key,
            resource_kind: op.resource_kind,
            resource_key: op.resource_key,
            action: op.action,
            spec_key: op.spec_key,
            accepted_at: op.accepted_at,
            completed_at: op.completed_at,
            outcome: op.outcome,
            diagnostic: op.diagnostic,
          },
          steps: steps
            .iter()
            .map(JobRunStepDb::to_public)
            .collect::<IoResult<Vec<_>>>()?,
        })
      })
    })
    .await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    models::{
      DistributedMutexDb, DistributedMutexPartial, OperationOwner, SystemState,
    },
    repositories::generic::{RepositoryCreate, RepositoryReadByTransform},
    utils::tests::{TestSystem, gen_test_system},
    vars,
  };
  use nanocl_stubs::job::JobPartial;

  fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}

  struct Fixture {
    system: TestSystem,
    job: nanocl_stubs::job::Job,
    mutexes: Vec<uuid::Uuid>,
  }

  impl Fixture {
    async fn new() -> Self {
      let system = gen_test_system(no_routes, vars::VERSION).await;
      let partial = JobPartial {
        name: format!("job-run-test-{}", uuid::Uuid::new_v4().simple()),
        secrets: Some(vec!["authored-secret-reference".into()]),
        containers: vec![Default::default(), Default::default()],
        ..Default::default()
      };
      let job =
        JobDb::create_with_spec(&partial, "v0.19", &system.state.inner.pool)
          .await
          .unwrap();
      Self {
        system,
        job,
        mutexes: Vec::new(),
      }
    }
    fn pool(&self) -> &Pool {
      &self.system.state.inner.pool
    }
    async fn refresh(&mut self) {
      self.job = JobDb::transform_read_by_pk(&self.job.spec.name, self.pool())
        .await
        .unwrap();
    }
    async fn operation(&mut self, action: &str) -> OperationDb {
      let mutex = DistributedMutexDb::create_from(
        DistributedMutexPartial {
          action: action.into(),
          resource_kind: "Job".into(),
          resource_key: self.job.spec.name.clone(),
          node_key: self.system.state.inner.config.hostname.clone(),
          acquired_by: "job-run-transaction-test".into(),
        },
        self.pool(),
      )
      .await
      .unwrap();
      self.mutexes.push(mutex.key);
      OperationDb::new(
        &OperationOwner {
          key: mutex.key,
          resource_kind: "Job".into(),
          resource_key: self.job.spec.name.clone(),
          incarnation_key: self.job.incarnation_key,
          node_name: mutex.node_key,
          owner_boot_id: "job-run-test-boot".into(),
          lock_device: 1,
          lock_inode: 2,
        },
        "Job",
        &self.job.spec.name,
        self.job.spec_key,
        action,
      )
    }
    async fn start(&mut self) -> crate::models::AcceptedJobRun {
      self.refresh().await;
      let operation = self.operation("start").await;
      let accepted = JobDb::accept_run(&self.job, operation, (), self.pool())
        .await
        .unwrap();
      assert!(
        JobDb::complete_start(&accepted.operation, (), self.pool())
          .await
          .unwrap()
      );
      self.job = accepted.job.clone();
      accepted
    }
    async fn success(&self, run: &JobRunDb, index: i32) {
      assert!(
        JobRunDb::begin_step(run, index, (), self.pool())
          .await
          .unwrap()
      );
      let container =
        format!("job-run-test-container-{}", uuid::Uuid::new_v4());
      assert!(
        JobRunDb::attach_container(run, index, &container, (), self.pool())
          .await
          .unwrap()
      );
      assert!(
        JobRunDb::update_step(
          run,
          index,
          "Running",
          None,
          None,
          (),
          self.pool()
        )
        .await
        .unwrap()
      );
      assert!(
        JobRunDb::update_step(
          run,
          index,
          "Succeeded",
          Some(0),
          None,
          (),
          self.pool()
        )
        .await
        .unwrap()
      );
    }
    async fn cleanup(self) {
      use crate::schema::{
        distributed_mutexes, object_process_statuses, resource_incarnations,
      };
      let name = self.job.spec.name.clone();
      let mutexes = self.mutexes.clone();
      run_query_owned(self.pool(), (), move |conn| {
        transaction(conn, |conn| {
          let incarnations = resource_incarnations::table
            .filter(resource_incarnations::resource_kind.eq("Job"))
            .filter(resource_incarnations::resource_key.eq(&name))
            .select(resource_incarnations::key)
            .load::<uuid::Uuid>(conn)?;
          let runs = job_runs::table
            .filter(job_runs::incarnation_key.eq_any(&incarnations))
            .select(job_runs::key)
            .load::<uuid::Uuid>(conn)?;
          diesel::delete(jobs::table.find(&name)).execute(conn)?;
          diesel::delete(object_process_statuses::table.find(&name))
            .execute(conn)?;
          diesel::delete(
            job_run_steps::table.filter(job_run_steps::run_key.eq_any(&runs)),
          )
          .execute(conn)?;
          diesel::delete(job_runs::table.filter(job_runs::key.eq_any(&runs)))
            .execute(conn)?;
          diesel::update(
            resource_incarnations::table
              .filter(resource_incarnations::key.eq_any(&incarnations)),
          )
          .set(
            resource_incarnations::current_operation_key.eq(None::<uuid::Uuid>),
          )
          .execute(conn)?;
          diesel::delete(
            operations::table
              .filter(operations::incarnation_key.eq_any(&incarnations)),
          )
          .execute(conn)?;
          diesel::delete(
            specs::table.filter(specs::incarnation_key.eq_any(&incarnations)),
          )
          .execute(conn)?;
          diesel::delete(
            resource_incarnations::table
              .filter(resource_incarnations::key.eq_any(&incarnations)),
          )
          .execute(conn)?;
          diesel::delete(
            distributed_mutexes::table
              .filter(distributed_mutexes::key.eq_any(&mutexes)),
          )
          .execute(conn)?;
          Ok(())
        })
      })
      .await
      .unwrap();
      self.system.state.wait_event_loop().await;
    }
  }

  #[ntex::test]
  async fn job_runs_keep_two_ordered_executions_and_pinned_history_after_recreation()
   {
    let mut fixture = Fixture::new().await;
    let first = fixture.start().await;
    fixture.success(&first.run, 0).await;
    assert!(
      JobRunDb::finish(&first.run, "Succeeded", None, (), fixture.pool())
        .await
        .is_err()
    );
    fixture.success(&first.run, 1).await;
    assert!(
      JobRunDb::finish(&first.run, "Succeeded", None, (), fixture.pool())
        .await
        .unwrap()
    );
    let second = fixture.start().await;
    assert_ne!(first.run.key, second.run.key);
    fixture.success(&second.run, 0).await;
    fixture.success(&second.run, 1).await;
    assert!(
      JobRunDb::finish(&second.run, "Succeeded", None, (), fixture.pool())
        .await
        .unwrap()
    );
    let list = JobDb::runs(
      &fixture.job.spec.name,
      &JobHistoryQuery::default(),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].key, second.run.key);
    let detail =
      JobDb::inspect_run(&fixture.job.spec.name, first.run.key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(
      detail
        .steps
        .iter()
        .map(|step| step.step_index)
        .collect::<Vec<_>>(),
      vec![0, 1]
    );
    assert!(detail.steps.iter().all(|step| step.exit_code == Some(0)));
    assert_eq!(detail.operation.outcome.as_deref(), Some("completed"));
    assert_eq!(
      detail.spec.secrets,
      Some(vec!["authored-secret-reference".into()])
    );
    fixture.refresh().await;
    let delete = fixture.operation("delete").await;
    JobDb::accept_control(
      &fixture.job,
      delete.clone(),
      Some(second.run.key),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      JobDb::delete_operation(
        &delete,
        Some(second.run.key),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let mut replacement: JobPartial = fixture.job.clone().into();
    replacement.containers.clear();
    replacement.metadata = Some(serde_json::json!({"replacement":true}));
    fixture.job =
      JobDb::create_with_spec(&replacement, "v0.19", fixture.pool())
        .await
        .unwrap();
    assert_ne!(fixture.job.incarnation_key, first.run.incarnation_key);
    assert_ne!(fixture.job.spec_key, first.run.spec_key);
    let retained =
      JobDb::inspect_run(&fixture.job.spec.name, first.run.key, fixture.pool())
        .await
        .unwrap();
    assert_eq!(retained.spec, detail.spec);
    assert_eq!(retained.run, detail.run);
    assert!(
      !JobRunDb::update_step(
        &first.run,
        0,
        "Failed",
        Some(1),
        None,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !JobDb::delete_operation(
        &delete,
        Some(second.run.key),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    fixture.refresh().await;
    assert_eq!(fixture.job.current_run_key, None);
    let page = JobDb::runs(
      &fixture.job.spec.name,
      &JobHistoryQuery {
        limit: Some(1),
        offset: Some(1),
      },
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(page[0].key, first.run.key);
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn job_runs_concurrent_admission_and_expired_mutex_cannot_overlap() {
    let mut fixture = Fixture::new().await;
    let manual = fixture.operation("start").await;
    // The resource mutex already serializes independent request owners. Race
    // duplicate dispatch through that same owner to exercise SQL admission too.
    let scheduled = manual.clone();
    let (manual, scheduled) = futures::join!(
      JobDb::accept_run(&fixture.job, manual, (), fixture.pool()),
      JobDb::accept_run(&fixture.job, scheduled, (), fixture.pool())
    );
    assert_ne!(manual.is_ok(), scheduled.is_ok());
    let accepted = manual.ok().or(scheduled.ok()).unwrap();
    JobDb::complete_start(&accepted.operation, (), fixture.pool())
      .await
      .unwrap();
    fixture.refresh().await;
    let status = fixture.job.status.clone();
    let rejected = fixture.operation("start").await;
    assert!(
      JobDb::accept_run(&fixture.job, rejected, (), fixture.pool())
        .await
        .is_err()
    );
    fixture.refresh().await;
    assert_eq!(fixture.job.status, status);
    assert_eq!(fixture.job.current_run_key, Some(accepted.run.key));
    assert_eq!(
      JobDb::runs(
        &fixture.job.spec.name,
        &JobHistoryQuery::default(),
        fixture.pool()
      )
      .await
      .unwrap()
      .len(),
      1
    );
    assert!(
      JobRunDb::finish(
        &accepted.run,
        "Interrupted",
        Some("Simulated loss before dispatch".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let detail = JobDb::inspect_run(
      &fixture.job.spec.name,
      accepted.run.key,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(detail.steps.iter().all(|step| step.exit_code.is_none()));
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn job_runs_cancellation_blocks_next_step_and_unknown_keeps_admission_closed()
   {
    let mut fixture = Fixture::new().await;
    let accepted = fixture.start().await;
    fixture.success(&accepted.run, 0).await;
    fixture.refresh().await;
    let stop = fixture.operation("stop").await;
    JobDb::accept_control(
      &fixture.job,
      stop.clone(),
      Some(accepted.run.key),
      (),
      fixture.pool(),
    )
    .await
    .unwrap();
    assert!(
      !JobRunDb::begin_step(&accepted.run, 1, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      !JobRunDb::can_execute(&accepted.run, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      !JobRunDb::finish(&accepted.run, "Succeeded", None, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      JobRunDb::finish(
        &accepted.run,
        "Interrupted",
        Some("Stopped between steps".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    JobDb::complete_control(&stop, "completed", None, (), fixture.pool())
      .await
      .unwrap();
    let next = fixture.start().await;
    assert!(
      JobRunDb::begin_step(&next.run, 0, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      JobRunDb::attach_container(
        &next.run,
        0,
        &format!("unknown-{}", next.run.key),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::mark_unknown(
        &next.run,
        "Simulated runtime observation failure".into(),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::finish(&next.run, "Interrupted", None, (), fixture.pool())
        .await
        .is_err()
    );
    fixture.refresh().await;
    let status = fixture.job.status.clone();
    let rejected = fixture.operation("start").await;
    assert!(
      JobDb::accept_run(&fixture.job, rejected, (), fixture.pool())
        .await
        .is_err()
    );
    fixture.refresh().await;
    assert_eq!(fixture.job.status, status);
    assert!(
      JobRunDb::update_step(
        &next.run,
        0,
        "Interrupted",
        None,
        Some("Confirmed stopped".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::finish(
        &next.run,
        "Interrupted",
        Some("Stopped after ownership loss".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let detail =
      JobDb::inspect_run(&fixture.job.spec.name, next.run.key, fixture.pool())
        .await
        .unwrap();
    assert!(detail.steps.iter().all(|step| step.exit_code.is_none()));
    fixture.cleanup().await;
  }

  #[ntex::test]
  async fn job_runs_accepted_creation_and_nonzero_failures_remain_visible() {
    let mut fixture = Fixture::new().await;
    let accepted = fixture.start().await;
    assert!(
      JobRunDb::begin_step(&accepted.run, 0, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      JobRunDb::update_step(
        &accepted.run,
        0,
        "Failed",
        None,
        Some("Container creation failed".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::finish(
        &accepted.run,
        "Failed",
        Some("Container creation failed".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let failed = JobDb::inspect_run(
      &fixture.job.spec.name,
      accepted.run.key,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(failed.steps[0].exit_code, None);
    assert_eq!(
      failed.steps[1].state,
      nanocl_stubs::job::JobStepState::Skipped
    );
    let second = fixture.start().await;
    assert!(
      JobRunDb::begin_step(&second.run, 0, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      JobRunDb::attach_container(
        &second.run,
        0,
        &format!("nonzero-{}", second.run.key),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::update_step(
        &second.run,
        0,
        "Running",
        None,
        None,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::update_step(
        &second.run,
        0,
        "Unknown",
        None,
        Some("Docker wait failed before exit was observed".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      JobRunDb::update_step(
        &second.run,
        0,
        "Failed",
        Some(23),
        None,
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    assert!(
      !JobRunDb::begin_step(&second.run, 1, (), fixture.pool())
        .await
        .unwrap()
    );
    assert!(
      JobRunDb::finish(
        &second.run,
        "Failed",
        Some("Step exited with status 23".into()),
        (),
        fixture.pool()
      )
      .await
      .unwrap()
    );
    let detail = JobDb::inspect_run(
      &fixture.job.spec.name,
      second.run.key,
      fixture.pool(),
    )
    .await
    .unwrap();
    assert_eq!(detail.steps[0].exit_code, Some(23));
    assert_eq!(
      detail.steps[0].error.as_deref(),
      Some("Docker wait failed before exit was observed")
    );
    assert_eq!(detail.steps[1].exit_code, None);
    fixture.cleanup().await;
  }
}
