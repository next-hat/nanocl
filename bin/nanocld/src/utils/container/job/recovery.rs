use std::time::Duration;

use nanocl_error::io::IoResult;
use nanocl_stubs::generic::GenericFilter;

use crate::{
  models::{JobDb, JobRunDb, OperationDb, SystemState},
  repositories::generic::*,
  utils,
};

/// No command is replayed after executor ownership is lost. Pending controls
/// are interrupted; retained runs are settled only from fresh Docker evidence.
pub(crate) async fn reconcile(state: &SystemState) -> IoResult<()> {
  let mut after = None;
  loop {
    let pending = OperationDb::pending_for_node(
      "Job",
      &state.inner.config.hostname,
      after,
      128,
      &state.inner.pool,
    )
    .await?;
    let done = pending.len() < 128;
    for operation in pending {
      after = Some(operation.key);
      let lock = super::lifecycle_lock(&operation.resource_key);
      let Ok(_serialized) = lock.try_lock_owned() else {
        continue;
      };
      let owner = match utils::operation::abandoned_owner(
        &operation.owner(),
        &state.inner.config.hostname,
        &state.inner.config.state_dir,
      ) {
        Ok(Some(owner)) => owner,
        Ok(None) => continue,
        Err(error) => {
          log::warn!(
            "Job control {} recovery deferred: {error}",
            operation.key
          );
          continue;
        }
      };
      if let Err(error) = JobDb::complete_control(&operation, "interrupted", Some("Daemon execution ownership was lost; control action was not replayed".into()), owner.clone(), &state.inner.pool).await {
        log::warn!("Job control {} interruption could not be recorded: {error}", operation.key);
      }
      if let Err(error) =
        utils::operation::cleanup_owner(&owner, &state.inner.pool).await
      {
        log::warn!(
          "Job control {} ownership cleanup deferred: {error}",
          operation.key
        );
      }
    }
    if done {
      break;
    }
  }

  for run in JobRunDb::active(&state.inner.pool).await? {
    let lock = super::lifecycle_lock(&run.job_name);
    let Ok(_serialized) = lock.try_lock_owned() else {
      continue;
    };
    let Some(operation) =
      OperationDb::get(run.operation_key, &state.inner.pool).await?
    else {
      log::error!("Job run {} has no retained initiating operation", run.key);
      continue;
    };
    let owner = match utils::operation::abandoned_owner(
      &operation.owner(),
      &state.inner.config.hostname,
      &state.inner.config.state_dir,
    ) {
      Ok(Some(owner)) => owner,
      Ok(None) => continue,
      Err(error) => {
        log::warn!("Job run {} recovery deferred: {error}", run.key);
        continue;
      }
    };
    let reason = "Daemon execution ownership was lost; unexecuted steps will not be launched".to_owned();
    match super::settle_observed(
      &run,
      "Interrupted",
      reason.clone(),
      &owner,
      state,
    )
    .await
    {
      Ok(_) => {}
      Err(error) => {
        JobRunDb::mark_unknown(
          &run,
          format!("{reason}; runtime observation unavailable: {error}"),
          owner.clone(),
          &state.inner.pool,
        )
        .await?;
      }
    }
    if let Err(error) =
      utils::operation::cleanup_owner(&owner, &state.inner.pool).await
    {
      log::warn!("Job run {} ownership cleanup deferred: {error}", run.key);
    }
  }
  reconcile_ttl(state).await
}

/// A daemon restart can lose an in-memory TTL timer. Walk live Jobs in bounded
/// pages and use the same incarnation/run-checked deletion as the timer.
async fn reconcile_ttl(state: &SystemState) -> IoResult<()> {
  let mut offset = 0;
  loop {
    let jobs = JobDb::transform_read_by(
      &GenericFilter::new().limit(100).offset(offset),
      &state.inner.pool,
    )
    .await?;
    let done = jobs.len() < 100;
    offset += jobs.len();
    for job in jobs {
      let (Some(ttl), Some(key)) = (job.spec.ttl, job.current_run_key) else {
        continue;
      };
      let run = JobRunDb::get(key, &state.inner.pool).await?;
      let Some(finished) = run.finished_at else {
        continue;
      };
      let elapsed = chrono::Utc::now()
        .naive_utc()
        .signed_duration_since(finished)
        .num_seconds();
      if elapsed < 0 || u64::try_from(elapsed).unwrap_or(0) < ttl as u64 {
        continue;
      }
      if let Err(error) = crate::tasks::job::delete_if_current(
        &job.spec.name,
        job.incarnation_key,
        key,
        state,
      )
      .await
      {
        log::debug!(
          "Job {} retained TTL cleanup deferred: {error}",
          job.spec.name
        );
      }
    }
    if done {
      break;
    }
  }
  Ok(())
}

pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  ntex::rt::spawn(async move {
    loop {
      if let Err(error) = reconcile(&state).await {
        log::warn!("Job reconciliation deferred: {error}");
      }
      ntex::time::sleep(Duration::from_secs(30)).await;
    }
  });
}
