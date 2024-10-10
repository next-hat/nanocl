use bollard_next::{
  container::{Config, StartContainerOptions, WaitContainerOptions},
  secret::HostConfig,
};
use futures::StreamExt;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  job::Job,
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{JobDb, ObjPsStatusDb, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Create process (container) for a job
///
async fn create_instance(
  name: &str,
  index: usize,
  container: &Config,
  state: &SystemState,
) -> IoResult<Process> {
  let mut container = container.clone();
  let mut labels = container.labels.unwrap_or_default();
  labels.insert("io.nanocl.j".to_owned(), name.to_owned());
  container.labels = Some(labels);
  let host_config = container.host_config.unwrap_or_default();
  container.host_config = Some(HostConfig {
    network_mode: Some(
      host_config.network_mode.unwrap_or("nanoclbr0".to_owned()),
    ),
    ..host_config
  });
  let short_id = utils::key::generate_short_id(6);
  let container_name = format!("{name}-{index}-{short_id}.j");
  super::process::create(
    &ProcessKind::Job,
    &container_name,
    name,
    &container,
    state,
  )
  .await
}

/// Create processes (container) for a job
///
pub async fn create_instances(
  job: &Job,
  state: &SystemState,
) -> IoResult<Vec<Process>> {
  let mut processes = Vec::new();
  for (index, container) in job.containers.iter().enumerate() {
    super::image::download(
      &container.image.clone().unwrap_or_default(),
      job.image_pull_secret.clone(),
      job.image_pull_policy.clone().unwrap_or_default(),
      job,
      state,
    )
    .await?;
    let process = create_instance(&job.name, index, container, state).await?;
    processes.push(process);
  }
  Ok(processes)
}

/// Start job instances
///
pub async fn start(key: &str, state: &SystemState) -> IoResult<()> {
  let job = JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let mut processes =
    ProcessDb::read_by_kind_key(&job.name, &state.inner.pool).await?;
  if processes.is_empty() {
    processes = create_instances(&job, state).await?;
  }
  ObjPsStatusDb::update_actual_status(
    key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  state
    .emit_normal_native_action_sync(&job, NativeEventAction::Start)
    .await;
  for process in processes {
    let _ = state
      .inner
      .docker_api
      .start_container(&process.key, None::<StartContainerOptions<String>>)
      .await;
    // We currently run a sequential order so we wait for the container to finish to start the next one.
    let mut stream = state.inner.docker_api.wait_container(
      &process.key,
      Some(WaitContainerOptions {
        condition: "not-running",
      }),
    );
    while let Some(stream) = stream.next().await {
      let result = stream
        .map_err(|err| IoError::interrupted("JobCreate", &format!("{err}")))?;
      if result.status_code == 0 {
        break;
      }
    }
  }
  Ok(())
}

/// Delete job instances and the job itself in the database
///
pub async fn delete(key: &str, state: &SystemState) -> IoResult<()> {
  let job = JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes = ProcessDb::read_by_kind_key(key, &state.inner.pool).await?;
  super::process::delete_instances(
    &processes
      .iter()
      .map(|p| p.key.clone())
      .collect::<Vec<String>>(),
    state,
  )
  .await?;
  log::debug!("JobDb::delete_by_pk({:?})", &job.name);
  JobDb::clear_by_pk(&job.name, &state.inner.pool).await?;
  if job.schedule.is_some() {
    utils::cron::remove_cron_rule(&job, state).await?;
  }
  state
    .emit_normal_native_action_sync(&job, NativeEventAction::Destroy)
    .await;
  Ok(())
}
