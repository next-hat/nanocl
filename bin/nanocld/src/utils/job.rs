use std::collections::HashMap;

use bollard_next::service::ContainerSummary;
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  RemoveContainerOptions,
};

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use nanocl_error::http::HttpError;
use nanocl_stubs::job::{Job, JobPartial};

use crate::repositories;
use crate::models::DaemonState;

/// ## List instances
///
/// List the job instances (containers) based on the job name
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<ContainerSummary>) - The containers have been listed
///   * [Err](HttpError) - The containers have not been listed
///
pub async fn list_instances(
  name: &str,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<ContainerSummary>, HttpError> {
  let label = format!("io.nanocl.job={name}");
  let mut filters: HashMap<&str, Vec<&str>> = HashMap::new();
  filters.insert("label", vec![&label]);
  let options = Some(ListContainersOptions {
    all: true,
    filters,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await?;
  Ok(containers)
}

async fn run_job(job: &Job, state: &DaemonState) -> Result<(), HttpError> {
  let containers = job.containers.clone();
  for mut container in containers {
    let mut labels = container.labels.clone().unwrap_or_default();
    labels.insert("io.nanocl.job".to_owned(), job.name.to_owned());
    container.labels = Some(labels);
    let container_instance = state
      .docker_api
      .create_container(
        None::<CreateContainerOptions<String>>,
        container.clone(),
      )
      .await?;
    state
      .docker_api
      .start_container(
        &container_instance.id,
        None::<StartContainerOptions<String>>,
      )
      .await?;
  }
  Ok(())
}

pub async fn create(
  item: &JobPartial,
  state: &DaemonState,
) -> Result<Job, HttpError> {
  let job = repositories::job::create(item, &state.pool).await?;
  run_job(&job, state).await?;
  Ok(job)
}

pub async fn list(state: &DaemonState) -> Result<Vec<Job>, HttpError> {
  let jobs = repositories::job::list(&state.pool).await?;
  Ok(jobs)
}

/// ## Delete by key
///
/// Delete a cargo by key with his given instances (containers).
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [force](Option<bool>) - Force the deletion of the cargo
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The cargo has been deleted
///   * [Err](HttpError) - The cargo has not been deleted
///
pub async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  let containers = list_instances(name, &state.docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      state
        .docker_api
        .remove_container(
          &container.id.unwrap_or_default(),
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  repositories::job::delete_by_name(&job.name, &state.pool).await?;
  Ok(())
}
