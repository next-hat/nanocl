use bollard_next::{container::Config, secret::HostConfig};
use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  job::Job,
  process::{Process, ProcessKind},
};

use crate::{models::SystemState, utils};

/// Create process (container) for a job
async fn create_job_instance(
  name: &str,
  index: usize,
  container: &Config,
  state: &SystemState,
) -> HttpResult<Process> {
  let mut container = container.clone();
  let mut labels = container.labels.clone().unwrap_or_default();
  labels.insert("io.nanocl.j".to_owned(), name.to_owned());
  container.labels = Some(labels);
  let host_config = container.host_config.clone().unwrap_or_default();
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
pub async fn create_job_instances(
  job: &Job,
  state: &SystemState,
) -> HttpResult<Vec<Process>> {
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
    let process =
      create_job_instance(&job.name, index, container, state).await?;
    processes.push(process);
  }
  Ok(processes)
}
