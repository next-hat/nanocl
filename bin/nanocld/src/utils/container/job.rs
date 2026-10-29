use std::{
  collections::HashMap,
  sync::{Arc, OnceLock},
  time::Duration,
};

use bollard_next::{
  models::{
    ContainerCreateBody as Config, ContainerInspectResponse, HostConfig,
  },
  query_parameters::{
    CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
  },
};
use futures::StreamExt;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{job::Job, process::ProcessKind, system::NativeEventAction};

use crate::{
  models::{
    AcceptedJobControl, AcceptedJobRun, JobDb, JobLifecycleLock,
    JobLifecycleLocks, JobRunDb, JobRuntimeObservation, OperationGuard,
    ProcessDb, SystemState,
  },
  repositories::generic::*,
  utils,
};

mod recovery;
pub(crate) use recovery::spawn as spawn_reconciliation;

pub(crate) const RUN_LABEL: &str = "io.nanocl.job.run";
pub(crate) const INCARNATION_LABEL: &str = "io.nanocl.job.incarnation";
pub(crate) const STEP_LABEL: &str = "io.nanocl.job.step";

pub(crate) fn lifecycle_lock(key: &str) -> JobLifecycleLock {
  static LOCKS: OnceLock<JobLifecycleLocks> = OnceLock::new();
  let mut locks = LOCKS
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|error| error.into_inner());
  locks.retain(|_, lock| lock.strong_count() > 0);
  if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
    return lock;
  }
  let lock = Arc::new(tokio::sync::Mutex::new(()));
  locks.insert(key.to_owned(), Arc::downgrade(&lock));
  lock
}

fn docker_error_kind(error: &bollard_next::errors::Error) -> String {
  match error {
    bollard_next::errors::Error::DockerResponseServerError {
      status_code,
      ..
    } => format!("Docker HTTP {status_code}"),
    _ => "Docker transport or response error".into(),
  }
}

fn create_rejected(error: &bollard_next::errors::Error) -> bool {
  matches!(
    error,
    bollard_next::errors::Error::DockerResponseServerError {
      status_code: 400 | 404 | 409,
      ..
    }
  )
}

fn conflict(message: &str) -> IoError {
  IoError::with_context(
    "Job execution",
    std::io::Error::new(std::io::ErrorKind::WouldBlock, message),
  )
}

fn network_modes(job: &Job) -> Vec<String> {
  job
    .spec
    .containers
    .iter()
    .map(super::network::container_network_mode)
    .collect()
}

fn runtime_step(
  run: &JobRunDb,
  runtime: &ContainerInspectResponse,
) -> IoResult<i32> {
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  let label = |key: &str| {
    labels
      .and_then(|labels| labels.get(key))
      .map(String::as_str)
  };
  if label("io.nanocl.kind") != Some("job")
    || label("io.nanocl.j") != Some(run.job_name.as_str())
    || label(RUN_LABEL) != Some(run.key.to_string().as_str())
    || label(INCARNATION_LABEL)
      != Some(run.incarnation_key.to_string().as_str())
  {
    return Err(conflict(
      "Docker container does not belong to this exact Job run",
    ));
  }
  label(STEP_LABEL)
    .and_then(|value| value.parse::<i32>().ok())
    .filter(|index| *index >= 0)
    .ok_or_else(|| {
      conflict("Job container step identity is missing or invalid")
    })
}

fn observation(
  runtime: &ContainerInspectResponse,
) -> IoResult<JobRuntimeObservation> {
  let observed = runtime
    .state
    .as_ref()
    .ok_or_else(|| conflict("Docker container state is unavailable"))?;
  if observed.running == Some(true) || observed.restarting == Some(true) {
    return Ok(JobRuntimeObservation::Running);
  }
  if observed.running != Some(false) {
    return Err(conflict("Docker container running state is unknown"));
  }
  let started = observed.started_at.as_deref().is_some_and(|time| {
    !time.starts_with("0001-")
      && chrono::DateTime::parse_from_rfc3339(time).is_ok()
  });
  if !started {
    return Ok(JobRuntimeObservation::NotStarted);
  }
  let finished = observed.finished_at.as_deref().is_some_and(|time| {
    !time.starts_with("0001-")
      && chrono::DateTime::parse_from_rfc3339(time).is_ok()
  });
  if !finished {
    return Err(conflict("Docker has not confirmed a finished execution"));
  }
  observed
    .exit_code
    .map(JobRuntimeObservation::Exited)
    .ok_or_else(|| conflict("Docker did not report the actual exit code"))
}

async fn runtime_config(
  job: &Job,
  run: &JobRunDb,
  index: i32,
  item: &Config,
  state: &SystemState,
) -> IoResult<Config> {
  let mut container = item.clone();
  let mut labels = container.labels.take().unwrap_or_default();
  labels.insert("io.nanocl".into(), "enabled".into());
  labels.insert("io.nanocl.kind".into(), "job".into());
  labels.insert("io.nanocl.j".into(), run.job_name.clone());
  labels.insert(RUN_LABEL.into(), run.key.to_string());
  labels.insert(INCARNATION_LABEL.into(), run.incarnation_key.to_string());
  labels.insert(STEP_LABEL.into(), index.to_string());
  container.labels = Some(labels);
  let env_secrets =
    utils::secret::load_env_secrets(&job.spec.secrets, state).await?;
  // Secret injection only affects this disposable runtime configuration. The
  // accepted immutable specification is never replaced with this value.
  let secret_dir = utils::secret::create_tls_secrets(
    &job.spec.name,
    &ProcessKind::Job,
    &job.spec.secrets,
    state,
  )
  .await?;
  container.env = Some(
    container
      .env
      .unwrap_or_default()
      .into_iter()
      .chain(env_secrets)
      .collect(),
  );
  let host = container.host_config.unwrap_or_default();
  let mut binds = host.binds.clone().unwrap_or_default();
  binds.push(format!("{secret_dir}/:/opt/nanocl.io/secrets"));
  container.host_config = Some(HostConfig {
    network_mode: Some(super::network::resolve_network_mode(host.network_mode)),
    binds: Some(binds),
    ..host
  });
  Ok(container)
}

/// Inventory uses retained run labels as well as immutable recorded IDs. It
/// catches a crash between Docker creation and recording the returned ID.
async fn inventory(
  run: &JobRunDb,
  state: &SystemState,
) -> IoResult<Vec<(i32, String, ContainerInspectResponse)>> {
  let steps = JobRunDb::steps(run.key, &state.inner.pool).await?;
  let containers = state
    .inner
    .docker_api
    .list_containers(Some(ListContainersOptions {
      all: true,
      filters: Some(HashMap::from([(
        "label".to_owned(),
        vec![
          format!("{RUN_LABEL}={}", run.key),
          format!("{INCARNATION_LABEL}={}", run.incarnation_key),
        ],
      )])),
      ..Default::default()
    }))
    .await
    .map_err(|error| {
      IoError::interrupted("Job runtime inventory", &docker_error_kind(&error))
    })?;
  let mut ids = containers
    .into_iter()
    .map(|container| {
      container
        .id
        .ok_or_else(|| conflict("Docker inventory has no immutable ID"))
    })
    .collect::<IoResult<Vec<_>>>()?;
  ids.extend(steps.iter().filter_map(|step| step.container_id.clone()));
  ids.sort();
  ids.dedup();
  let mut observed = Vec::new();
  let mut positions = HashMap::new();
  for id in ids {
    let runtime =
      match state.inner.docker_api.inspect_container(&id, None).await {
        Ok(runtime) => runtime,
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => continue,
        Err(error) => {
          return Err(IoError::interrupted(
            "Job immutable container inspection",
            &docker_error_kind(&error),
          ));
        }
      };
    if runtime.id.as_deref() != Some(id.as_str()) {
      return Err(conflict(
        "Docker inspection changed the immutable container ID",
      ));
    }
    let index = runtime_step(run, &runtime)?;
    let step = steps
      .iter()
      .find(|step| step.step_index == index)
      .ok_or_else(|| {
        conflict(
          "Docker container refers to a step outside the accepted definition",
        )
      })?;
    if step
      .container_id
      .as_deref()
      .is_some_and(|expected| expected != id)
      || positions.insert(index, id.clone()).is_some()
    {
      return Err(conflict(
        "More than one container identifies the same Job run step",
      ));
    }
    observed.push((index, id, runtime));
  }
  observed.sort_by_key(|(index, _, _)| *index);
  Ok(observed)
}

async fn record_observation(
  run: &JobRunDb,
  index: i32,
  id: &str,
  runtime: &ContainerInspectResponse,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<JobRuntimeObservation> {
  JobRunDb::attach_container(run, index, id, owner.clone(), &state.inner.pool)
    .await?;
  let observed = observation(runtime)?;
  let (step_state, code) = match observed {
    JobRuntimeObservation::Running => ("Running", None),
    JobRuntimeObservation::NotStarted => return Ok(observed),
    JobRuntimeObservation::Exited(0) => ("Succeeded", Some(0)),
    JobRuntimeObservation::Exited(code) => ("Failed", Some(code)),
  };
  JobRunDb::update_step(
    run,
    index,
    step_state,
    code,
    None,
    owner.clone(),
    &state.inner.pool,
  )
  .await?;
  // Process rows are a current runtime view. Their removal cannot delete the
  // durable step IDs/results kept separately in job_run_steps.
  utils::system::sync_process(&run.job_name, "job", runtime, state).await?;
  Ok(observed)
}

async fn finish(
  run: &JobRunDb,
  outcome: &str,
  reason: Option<String>,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<()> {
  if !JobRunDb::finish(run, outcome, reason, owner.clone(), &state.inner.pool)
    .await?
  {
    return Ok(());
  }
  if let Ok(job) =
    JobDb::transform_read_by_pk(&run.job_name, &state.inner.pool).await
    && job.incarnation_key == run.incarnation_key
    && job.current_run_key == Some(run.key)
  {
    let action = match outcome {
      "Succeeded" => NativeEventAction::Finish,
      "Interrupted" => NativeEventAction::Stop,
      _ => NativeEventAction::Fail,
    };
    state.emit_normal_native_action_sync(&job, action).await;
    schedule_ttl(&job, run.key, state);
  }
  Ok(())
}

fn schedule_ttl(job: &Job, run: uuid::Uuid, state: &SystemState) {
  let Some(ttl) = job.spec.ttl else {
    return;
  };
  let job = job.clone();
  let state = state.clone();
  ntex::rt::spawn(async move {
    ntex::time::sleep(Duration::from_secs(ttl as u64)).await;
    if let Err(error) = crate::tasks::job::delete_if_current(
      &job.spec.name,
      job.incarnation_key,
      run,
      &state,
    )
    .await
    {
      log::debug!("Job TTL cleanup deferred for run {run}: {error}");
    }
  });
}

/// Settle only after fresh evidence proves no runtime is still executing. A
/// stopped first step never establishes success for an unexecuted later step.
async fn settle_observed(
  run: &JobRunDb,
  fallback: &str,
  reason: String,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<bool> {
  let observed = inventory(run, state).await?;
  let mut running = false;
  let mut uncertain_creation = false;
  let present = observed
    .iter()
    .map(|(_, id, _)| id.clone())
    .collect::<std::collections::HashSet<_>>();
  for (index, id, runtime) in observed {
    match record_observation(run, index, &id, &runtime, owner, state).await? {
      JobRuntimeObservation::Running => running = true,
      JobRuntimeObservation::NotStarted => {
        JobRunDb::update_step(
          run,
          index,
          fallback,
          None,
          Some(reason.clone()),
          owner.clone(),
          &state.inner.pool,
        )
        .await?;
      }
      JobRuntimeObservation::Exited(_) => {}
    }
  }
  for step in JobRunDb::steps(run.key, &state.inner.pool).await? {
    if step.finished_at.is_none()
      && step
        .container_id
        .as_ref()
        .is_some_and(|id| !present.contains(id))
    {
      JobRunDb::update_step(
        run,
        step.step_index,
        "Interrupted",
        None,
        Some("Container is absent; no exit code was observed".into()),
        owner.clone(),
        &state.inner.pool,
      )
      .await?;
    } else if step.finished_at.is_none()
      && step.container_id.is_none()
      && matches!(step.state.as_str(), "Creating" | "Unknown" | "Running")
      && run.cancellation_requested
    {
      JobRunDb::update_step(
        run,
        step.step_index,
        "Interrupted",
        None,
        Some("Cancelled before any container was observed".into()),
        owner.clone(),
        &state.inner.pool,
      )
      .await?;
    } else if fallback == "Interrupted"
      && step.finished_at.is_none()
      && step.container_id.is_none()
      && matches!(step.state.as_str(), "Creating" | "Unknown" | "Running")
    {
      uncertain_creation = true;
    } else if fallback == "Failed"
      && step.finished_at.is_none()
      && step.container_id.is_none()
      && step.error.is_some()
    {
      JobRunDb::update_step(
        run,
        step.step_index,
        "Failed",
        None,
        step.error.clone(),
        owner.clone(),
        &state.inner.pool,
      )
      .await?;
    }
  }
  if running || uncertain_creation {
    JobRunDb::mark_unknown(run, reason, owner.clone(), &state.inner.pool)
      .await?;
    return Ok(false);
  }
  let steps = JobRunDb::steps(run.key, &state.inner.pool).await?;
  let all_succeeded = steps
    .iter()
    .all(|step| step.state == "Succeeded" && step.exit_code == Some(0));
  let failed = steps.iter().any(|step| {
    step.state == "Failed" || step.exit_code.is_some_and(|code| code != 0)
  });
  let outcome = if fallback == "Interrupted" && run.cancellation_requested {
    "Interrupted"
  } else if fallback == "Failed" {
    "Failed"
  } else if all_succeeded {
    "Succeeded"
  } else if failed {
    "Failed"
  } else {
    fallback
  };
  finish(run, outcome, Some(reason), owner, state).await?;
  Ok(true)
}

async fn fail_execution(
  run: &JobRunDb,
  index: Option<i32>,
  reason: String,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<()> {
  if let Some(index) = index {
    JobRunDb::update_step(
      run,
      index,
      "Unknown",
      None,
      Some(reason.clone()),
      owner.clone(),
      &state.inner.pool,
    )
    .await?;
  }
  match settle_observed(run, "Failed", reason.clone(), owner, state).await {
    Ok(_) => Ok(()),
    Err(observation_error) => {
      JobRunDb::mark_unknown(
        run,
        format!(
          "{reason}; runtime observation unavailable: {observation_error}"
        ),
        owner.clone(),
        &state.inner.pool,
      )
      .await?;
      Ok(())
    }
  }
}

/// Executes only the admitted immutable revision. No historical container is
/// restarted, and no step is launched after cancellation or ownership loss.
pub(crate) async fn execute(
  accepted: &AcceptedJobRun,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<()> {
  let run = &accepted.run;
  let job = &accepted.job;
  if super::network::ensure_networks(network_modes(job), state)
    .await
    .is_err()
  {
    let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
    return fail_execution(
      run,
      None,
      "Job network preflight failed".into(),
      owner,
      state,
    )
    .await;
  }
  for (position, container) in job.spec.containers.iter().enumerate() {
    let index =
      i32::try_from(position).map_err(|_| conflict("Too many Job steps"))?;
    {
      let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
      if !JobRunDb::begin_step(run, index, owner.clone(), &state.inner.pool)
        .await?
      {
        return Ok(());
      }
    }
    let prepared = super::image::download(
      &container.image.clone().unwrap_or_default(),
      job.spec.image_pull_secret.clone(),
      job.spec.image_pull_policy.clone().unwrap_or_default(),
      job,
      state,
    )
    .await;
    match prepared {
      Ok(()) => (),
      Err(_) => {
        let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
        return fail_execution(
          run,
          Some(index),
          "Job image preparation failed".into(),
          owner,
          state,
        )
        .await;
      }
    };
    let id = {
      let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
      if !JobRunDb::can_execute(run, owner.clone(), &state.inner.pool).await? {
        return Ok(());
      }
      let config = match runtime_config(job, run, index, container, state).await
      {
        Ok(config) => config,
        Err(_) => {
          return fail_execution(
            run,
            Some(index),
            "Job secret or runtime configuration preparation failed".into(),
            owner,
            state,
          )
          .await;
        }
      };
      let name = format!(
        "{}-{index}-{}.j",
        run.job_name,
        utils::key::generate_short_id(6)
      );
      let created = state
        .inner
        .docker_api
        .create_container(
          Some(CreateContainerOptions {
            name: Some(name),
            ..Default::default()
          }),
          config,
        )
        .await;
      let id = match created {
        Ok(created) => created.id,
        Err(error) if create_rejected(&error) => {
          let reason = format!(
            "Container creation rejected ({})",
            docker_error_kind(&error)
          );
          // Docker rejected this create request before creating a container.
          // No exit code or start timestamp exists for this failed step.
          JobRunDb::update_step(
            run,
            index,
            "Failed",
            None,
            Some(reason.clone()),
            owner.clone(),
            &state.inner.pool,
          )
          .await?;
          return finish(run, "Failed", Some(reason), owner, state).await;
        }
        Err(error) => {
          let reason = format!(
            "Container creation outcome is unconfirmed ({})",
            docker_error_kind(&error)
          );
          JobRunDb::update_step(
            run,
            index,
            "Unknown",
            None,
            Some(reason.clone()),
            owner.clone(),
            &state.inner.pool,
          )
          .await?;
          JobRunDb::mark_unknown(run, reason, owner.clone(), &state.inner.pool)
            .await?;
          return Ok(());
        }
      };
      if !JobRunDb::attach_container(
        run,
        index,
        &id,
        owner.clone(),
        &state.inner.pool,
      )
      .await?
      {
        return Err(conflict(
          "Created container could not be associated with its exact run",
        ));
      }
      if !JobRunDb::can_execute(run, owner.clone(), &state.inner.pool).await? {
        return Ok(());
      }
      if let Err(error) = state
        .inner
        .docker_api
        .start_container(&id, None::<StartContainerOptions>)
        .await
      {
        return fail_execution(
          run,
          Some(index),
          format!("Container start failed ({})", docker_error_kind(&error)),
          owner,
          state,
        )
        .await;
      }
      JobRunDb::update_step(
        run,
        index,
        "Running",
        None,
        None,
        owner.clone(),
        &state.inner.pool,
      )
      .await?;
      if let Ok(runtime) =
        state.inner.docker_api.inspect_container(&id, None).await
        && utils::system::sync_process(&run.job_name, "job", &runtime, state)
          .await
          .is_err()
      {
        // Process inventory is disposable; its failure must not abandon the
        // wait observer after Docker has already started the command.
        log::warn!(
          "Job process observation refresh deferred for run {} container {id}",
          run.key
        );
      }
      state
        .emit_normal_native_action_sync(job, NativeEventAction::Start)
        .await;
      id
    };
    let mut waits = state.inner.docker_api.wait_container(
      &id,
      Some(WaitContainerOptions {
        condition: "not-running".to_owned(),
      }),
    );
    let waited = waits.next().await;
    drop(waits);
    let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
    let current = JobRunDb::get(run.key, &state.inner.pool).await?;
    if current.finished_at.is_some() || current.cancellation_requested {
      return Ok(());
    }
    let result = match waited {
      Some(Ok(exit))
        if exit
          .error
          .as_ref()
          .and_then(|error| error.message.as_ref())
          .is_some_and(|message| !message.is_empty()) =>
      {
        Err("Docker wait reported an execution error".into())
      }
      Some(Ok(exit)) => {
        match state.inner.docker_api.inspect_container(&id, None).await {
          Ok(runtime) if runtime_step(run, &runtime)? == index => {
            match observation(&runtime)? {
              JobRuntimeObservation::Exited(code)
                if code == exit.status_code =>
              {
                Ok(code)
              }
              _ => Err(
                "Docker did not confirm that the waited container stopped"
                  .into(),
              ),
            }
          }
          Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          }) => Ok(exit.status_code),
          _ => Err("Docker wait result could not be confirmed".into()),
        }
      }
      Some(Err(bollard_next::errors::Error::DockerContainerWaitError {
        code,
        ..
      })) => match state.inner.docker_api.inspect_container(&id, None).await {
        Ok(runtime) if runtime_step(run, &runtime)? == index => {
          match observation(&runtime)? {
            JobRuntimeObservation::Exited(observed) if observed == code => {
              Ok(code)
            }
            _ => Err(
              "Docker wait exit was followed by an unconfirmed runtime state"
                .into(),
            ),
          }
        }
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => Ok(code),
        _ => {
          JobRunDb::update_step(run, index, "Failed", Some(code), Some(format!("Container exited with code {code}; runtime confirmation is unavailable")), owner.clone(), &state.inner.pool).await?;
          Err("Docker wait exit was observed but runtime confirmation is unavailable".into())
        }
      },
      // A transport error is not an execution result. The common failure
      // observer records any subsequently confirmed exit, while preserving the
      // wait error and preventing the remaining steps from being launched.
      Some(Err(error)) => Err(format!(
        "Container wait failed ({})",
        docker_error_kind(&error)
      )),
      None => Err("Docker wait ended without an execution result".to_owned()),
    };
    let code = match result {
      Ok(code) => code,
      Err(error) => {
        return fail_execution(run, Some(index), error, owner, state).await;
      }
    };
    JobRunDb::update_step(
      run,
      index,
      if code == 0 { "Succeeded" } else { "Failed" },
      Some(code),
      if code == 0 {
        None
      } else {
        Some(format!("Container exited with code {code}"))
      },
      owner.clone(),
      &state.inner.pool,
    )
    .await?;
    if let Ok(runtime) =
      state.inner.docker_api.inspect_container(&id, None).await
      && runtime_step(run, &runtime)? == index
      && let Err(error) =
        utils::system::sync_process(&run.job_name, "job", &runtime, state).await
    {
      log::warn!(
        "Job process observation refresh deferred for {}: {error}",
        run.key
      );
    }
    if code != 0 {
      return finish(
        run,
        "Failed",
        Some(format!("Step {index} exited with code {code}")),
        owner,
        state,
      )
      .await;
    }
  }
  let _serialized = lifecycle_lock(&run.job_name).lock_owned().await;
  if JobRunDb::can_execute(run, owner.clone(), &state.inner.pool).await? {
    finish(run, "Succeeded", None, owner, state).await?;
  }
  Ok(())
}

/// Caller holds the same local lifecycle lock as container create/start. The
/// durable cancellation flag was committed before this method was dispatched.
pub(crate) async fn control(
  accepted: &AcceptedJobControl,
  owner: &OperationGuard,
  state: &SystemState,
) -> IoResult<bool> {
  if let Some(run) = &accepted.run {
    let runtimes = inventory(run, state).await?;
    for (index, id, runtime) in runtimes {
      if observation(&runtime)? == JobRuntimeObservation::Running {
        // Bollard maps HTTP 304 to AlreadyStopped; both successful results
        // still require the fresh inspection below before settling the run.
        state
          .inner
          .docker_api
          .stop_container(&id, None::<StopContainerOptions>)
          .await
          .map_err(|error| {
            IoError::interrupted(
              "Stop Job container",
              &docker_error_kind(&error),
            )
          })?;
      }
      let confirmed =
        match state.inner.docker_api.inspect_container(&id, None).await {
          Ok(runtime) => runtime,
          Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          }) => continue,
          Err(error) => {
            return Err(IoError::interrupted(
              "Confirm Job container stopped",
              &docker_error_kind(&error),
            ));
          }
        };
      if runtime_step(run, &confirmed)? != index
        || record_observation(run, index, &id, &confirmed, owner, state).await?
          == JobRuntimeObservation::Running
      {
        return Err(conflict("Job container stop was not confirmed"));
      }
    }
    if run.finished_at.is_none()
      && !settle_observed(
        run,
        "Interrupted",
        format!("Job {} requested", accepted.operation.action),
        owner,
        state,
      )
      .await?
    {
      return Err(conflict("Job runtime stop remains unconfirmed"));
    }
    if accepted.operation.action == "delete" {
      for (_, id, _) in inventory(run, state).await? {
        match state
          .inner
          .docker_api
          .remove_container(
            &id,
            Some(RemoveContainerOptions {
              force: false,
              ..Default::default()
            }),
          )
          .await
        {
          Ok(())
          | Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          }) => {}
          Err(error) => {
            return Err(IoError::interrupted(
              "Remove Job container",
              &docker_error_kind(&error),
            ));
          }
        }
        ProcessDb::del_by_pk(&id, &state.inner.pool).await?;
      }
    }
  }
  if accepted.operation.action == "delete" {
    if accepted.job.spec.schedule.is_some() {
      utils::cron::remove_cron_rule(&accepted.job, state).await?;
    }
    JobDb::delete_operation(
      &accepted.operation,
      accepted.job.current_run_key,
      owner.clone(),
      &state.inner.pool,
    )
    .await
  } else {
    JobDb::complete_control(
      &accepted.operation,
      "completed",
      None,
      owner.clone(),
      &state.inner.pool,
    )
    .await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bollard_next::models::ContainerState;

  fn container_on(network: &str) -> Config {
    Config {
      host_config: Some(HostConfig {
        network_mode: Some(network.to_owned()),
        ..Default::default()
      }),
      ..Default::default()
    }
  }

  #[test]
  fn job_create_rejection_excludes_server_and_transport_uncertainty() {
    for status_code in [400, 404, 409, 408, 429, 500, 503] {
      let error = bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: "secret-value-must-not-persist".into(),
      };
      assert_eq!(
        create_rejected(&error),
        matches!(status_code, 400 | 404 | 409)
      );
      assert!(!docker_error_kind(&error).contains("secret-value"));
    }
    assert!(!create_rejected(
      &bollard_next::errors::Error::RequestTimeoutError
    ));
  }

  #[test]
  fn network_preflight_includes_every_job_container_mode() {
    let job = Job {
      spec: nanocl_stubs::job::JobSpec {
        containers: vec![
          container_on("private-api"),
          container_on("private-db"),
        ],
        ..Default::default()
      },
      ..Default::default()
    };
    assert_eq!(
      network_modes(&job),
      vec!["private-api".to_owned(), "private-db".to_owned()]
    );
  }

  #[test]
  fn created_container_default_exit_code_is_not_success() {
    let runtime = ContainerInspectResponse {
      state: Some(ContainerState {
        running: Some(false),
        exit_code: Some(0),
        started_at: Some("0001-01-01T00:00:00Z".into()),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert_eq!(
      observation(&runtime).unwrap(),
      JobRuntimeObservation::NotStarted
    );
  }

  #[test]
  fn runtime_observation_requires_confirmed_exit_and_preserves_real_code() {
    let mut runtime = ContainerInspectResponse {
      state: Some(ContainerState {
        running: Some(false),
        exit_code: Some(7),
        started_at: Some("2026-10-27T18:00:00Z".into()),
        finished_at: Some("2026-10-27T18:00:01Z".into()),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert_eq!(
      observation(&runtime).unwrap(),
      JobRuntimeObservation::Exited(7)
    );
    runtime.state.as_mut().unwrap().finished_at = None;
    assert!(observation(&runtime).is_err());
    runtime.state.as_mut().unwrap().running = Some(true);
    assert_eq!(
      observation(&runtime).unwrap(),
      JobRuntimeObservation::Running
    );
  }
}

#[cfg(test)]
mod runtime_tests;
