use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{IoError, IoResult},
};
use nanocl_stubs::{
  job::Job,
  system::{EventActorKind, EventKind, NativeEventAction},
};

use super::generic::ObjTask;
use crate::{
  models::{JobDb, JobRunDb, OperationDb, OperationGuard, SystemState},
  repositories::generic::*,
  utils,
};

impl ObjTask for JobDb {
  fn get_kind() -> String {
    EventActorKind::Job.to_string()
  }
}

fn http_error(error: IoError) -> HttpError {
  if error.inner.kind() == std::io::ErrorKind::WouldBlock {
    HttpError::conflict(error.to_string())
  } else {
    error.into()
  }
}

async fn prepare(
  key: &str,
  action: &str,
  state: &SystemState,
) -> IoResult<(Job, OperationDb, OperationGuard)> {
  let job = JobDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let mutex = JobDb::create_mutex(key, action, state).await?;
  let (owner, guard) = match utils::operation::create_owner(
    mutex.key,
    "Job",
    key,
    job.incarnation_key,
    &state.inner.config.hostname,
    &state.inner.config.state_dir,
  ) {
    Ok(owner) => owner,
    Err(error) => {
      JobDb::delete_mutex(&mutex.key, state).await?;
      return Err(error);
    }
  };
  let operation = OperationDb::new(&owner, "Job", key, job.spec_key, action);
  Ok((job, operation, guard))
}

async fn cleanup(guard: &OperationGuard, state: &SystemState) {
  if let Err(error) =
    utils::operation::cleanup_owner(guard, &state.inner.pool).await
  {
    log::warn!("Job operation owner cleanup deferred: {error}");
  }
}

async fn rejected(guard: &OperationGuard, state: &SystemState) {
  match utils::operation::cleanup_owner(guard, &state.inner.pool).await {
    Ok(true) => {
      if let Err(error) = JobDb::delete_mutex(&guard.owner.key, state).await {
        log::warn!("Job rejected operation mutex cleanup: {error}");
      }
    }
    Ok(false) => {}
    Err(error) => {
      log::warn!("Job rejected acceptance remains recoverable: {error}")
    }
  }
}

/// Persist acceptance before dispatch. The detached owner survives request
/// cancellation; its file remains held for the entire execution.
pub(crate) async fn start(key: &str, state: &SystemState) -> HttpResult<()> {
  start_if_incarnation(key, None, state).await
}

pub(crate) async fn start_if_incarnation(
  key: &str,
  expected: Option<uuid::Uuid>,
  state: &SystemState,
) -> HttpResult<()> {
  let lock = utils::container::job::lifecycle_lock(key);
  // Serialize admission through the preceding action's event publication.
  // Durable acceptance still rejects an unfinished run or pending operation.
  let serialized = lock.lock_owned().await;
  let key = key.to_owned();
  let state = state.clone();
  let (reply, response) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let (job, operation, owner) = match prepare(&key, "start", &state).await {
      Ok(prepared) => prepared,
      Err(error) => {
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    if expected.is_some_and(|incarnation| incarnation != job.incarnation_key) {
      rejected(&owner, &state).await;
      let _ = reply.send(Err(HttpError::conflict(
        "Scheduled Job incarnation was replaced",
      )));
      return;
    }
    let accepted = match JobDb::accept_run(
      &job,
      operation,
      owner.clone(),
      &state.inner.pool,
    )
    .await
    {
      Ok(accepted) => accepted,
      Err(error) => {
        rejected(&owner, &state).await;
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    // This task now owns the accepted execution. Completing this short control
    // operation frees stop/delete while the workload continues under its run ID.
    let completed = JobDb::complete_start(
      &accepted.operation,
      owner.clone(),
      &state.inner.pool,
    )
    .await;
    match completed {
      Ok(true) => {
        state
          .emit_normal_native_action_sync(
            &accepted.job,
            NativeEventAction::Starting,
          )
          .await;
        // A stop/delete accepted after this reply must not publish its event
        // before the delayed Starting event of the run it just cancelled.
        drop(serialized);
        let _ = reply.send(Ok(()));
        if let Err(error) =
          utils::container::job::execute(&accepted, &owner, &state).await
        {
          log::error!(
            "Job run {} execution remains recoverable: {error}",
            accepted.run.key
          );
          let _ = JobRunDb::mark_unknown(
            &accepted.run,
            error.to_string(),
            owner.clone(),
            &state.inner.pool,
          )
          .await;
        }
      }
      Ok(false) => {
        let _ = reply.send(Err(HttpError::conflict(
          "Job start operation lost ownership before dispatch",
        )));
      }
      Err(error) => {
        let _ = reply.send(Err(http_error(error)));
      }
    }
    cleanup(&owner, &state).await;
  });
  response.await.map_err(|_| {
    HttpError::internal_server_error(
      "Job acceptance owner stopped before replying",
    )
  })?
}

async fn control(
  key: &str,
  action: &'static str,
  expected: Option<(uuid::Uuid, Option<uuid::Uuid>)>,
  wait_for_completion: bool,
  state: &SystemState,
) -> HttpResult<Job> {
  let lock = utils::container::job::lifecycle_lock(key);
  let serialized = lock.lock_owned().await;
  let key = key.to_owned();
  let state = state.clone();
  let (reply, response) = tokio::sync::oneshot::channel();
  ntex::rt::spawn(async move {
    let prepared = prepare(&key, action, &state).await;
    let (job, operation, owner) = match prepared {
      Ok(prepared) => prepared,
      Err(error) => {
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    if expected.is_some_and(|(incarnation, run)| {
      job.incarnation_key != incarnation || job.current_run_key != run
    }) {
      rejected(&owner, &state).await;
      let _ = reply.send(Err(HttpError::conflict(
        "Job execution changed before the control request was accepted",
      )));
      return;
    }
    let accepted = match JobDb::accept_control(
      &job,
      operation,
      job.current_run_key,
      owner.clone(),
      &state.inner.pool,
    )
    .await
    {
      Ok(accepted) => accepted,
      Err(error) => {
        rejected(&owner, &state).await;
        let _ = reply.send(Err(http_error(error)));
        return;
      }
    };
    let completion = if wait_for_completion {
      Some(reply)
    } else {
      let _ = reply.send(Ok(accepted.job.clone()));
      None
    };
    let result = utils::container::job::control(&accepted, &owner, &state)
      .await
      .and_then(|completed| {
        if completed {
          Ok(())
        } else {
          Err(IoError::interrupted(
            "Job control",
            "Operation completion was not recorded",
          ))
        }
      });
    if let Err(error) = &result {
      log::warn!("Job {} {} failed: {error}", accepted.job.spec.name, action);
      if let Some(run) = &accepted.run {
        let _ = JobRunDb::mark_unknown(
          run,
          error.to_string(),
          owner.clone(),
          &state.inner.pool,
        )
        .await;
      }
      let _ = JobDb::complete_control(
        &accepted.operation,
        "failed",
        Some(error.to_string()),
        owner.clone(),
        &state.inner.pool,
      )
      .await;
    }
    let action = if action == "delete" {
      NativeEventAction::Destroy
    } else {
      NativeEventAction::Stop
    };
    match &result {
      Ok(()) => {
        state
          .emit_normal_native_action_sync(&accepted.job, action)
          .await;
      }
      Err(error) => {
        state
          .emit_action_sync(
            &accepted.job.clone().into(),
            action,
            EventKind::Error,
            "state_sync",
            Some(error.to_string()),
            None,
          )
          .await;
      }
    }
    // Publish the predecessor's terminal event before a successor can publish
    // Starting. Event-driven follow-ups wait for this short admission boundary.
    drop(serialized);
    cleanup(&owner, &state).await;
    if let Some(reply) = completion {
      let _ = reply.send(result.map(|()| accepted.job).map_err(http_error));
    }
  });
  response.await.map_err(|_| {
    HttpError::internal_server_error(
      "Job control owner stopped before replying",
    )
  })?
}

pub(crate) async fn stop(key: &str, state: &SystemState) -> HttpResult<()> {
  control(key, "stop", None, false, state).await.map(|_| ())
}

pub(crate) async fn delete(key: &str, state: &SystemState) -> HttpResult<Job> {
  control(key, "delete", None, false, state).await
}

pub(crate) async fn delete_if_current(
  key: &str,
  incarnation: uuid::Uuid,
  run: uuid::Uuid,
  state: &SystemState,
) -> HttpResult<Job> {
  delete_if_identity(key, incarnation, Some(run), state).await
}

/// A normal caller can pin the exact inspected identity, including no run yet.
pub(crate) async fn delete_if_identity(
  key: &str,
  incarnation: uuid::Uuid,
  run: Option<uuid::Uuid>,
  state: &SystemState,
) -> HttpResult<Job> {
  control(key, "delete", Some((incarnation, run)), false, state).await
}

/// Replace only after the inspected incarnation's runtime and cron are removed.
pub(crate) async fn delete_for_replacement(
  expected: &Job,
  state: &SystemState,
) -> HttpResult<()> {
  control(
    &expected.spec.name,
    "delete",
    Some((expected.incarnation_key, expected.current_run_key)),
    true,
    state,
  )
  .await
  .map(|_| ())
}
