use std::collections::HashMap;

use ntex::util::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use futures_util::stream::{FuturesUnordered, select_all};
use bollard_next::service::{ContainerSummary, ContainerInspectResponse};
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  RemoveContainerOptions, LogsOptions, WaitContainerOptions,
};

use nanocl_error::http::HttpError;
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::job::{
  Job, JobPartial, JobInspect, JobLogOutput, JobWaitResponse, WaitCondition,
};

use crate::repositories;
use crate::models::DaemonState;

use super::stream::transform_stream;

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

/// ## Run job
///
/// Run a job in sequence based on the job definition
///
/// ## Arguments
///
/// * [job](Job) - The job
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - The job is running
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
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

/// ## Create
///
/// Create a job and run it
///
/// ## Arguments
///
/// * [item](JobPartial) - The job partial
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [Job](Job) has been created
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
pub async fn create(
  item: &JobPartial,
  state: &DaemonState,
) -> Result<Job, HttpError> {
  let job = repositories::job::create(item, &state.pool).await?;
  Ok(job)
}

/// ## Start by name
///
/// Start a job by name
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [Job](Job) has been started
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
pub async fn start_by_name(
  name: &str,
  state: &DaemonState,
) -> Result<Job, HttpError> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  run_job(&job, state).await?;
  Ok(job)
}

/// ## List
///
/// List all jobs
///
/// ## Arguments
///
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [Vector](Vec) of [Job](Job)
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
pub async fn list(state: &DaemonState) -> Result<Vec<Job>, HttpError> {
  let jobs = repositories::job::list(&state.pool).await?;
  Ok(jobs)
}

/// ## Delete by name
///
/// Delete a job by key with his given instances (containers).
///
/// ## Arguments
///
/// * [key](str) - The job key
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - The job has been deleted
///   * [Err](Err) - [Http error](HttpError) Something went wrong
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

/// ## Inspect by name
///
/// Inspect a job by name and return a detailed view of the job
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [JobInspect](JobInspect) has been returned
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
pub async fn inspect_by_name(
  name: &str,
  state: &DaemonState,
) -> Result<JobInspect, HttpError> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  let node =
    repositories::node::find_by_name(&state.config.hostname, &state.pool)
      .await?;
  let mut instance_success = 0;
  let container_inspects = list_instances(name, &state.docker_api)
    .await?
    .into_iter()
    .map(|container| async {
      let container_inspect = state
        .docker_api
        .inspect_container(&container.id.clone().unwrap_or_default(), None)
        .await?;
      Ok::<_, HttpError>((
        container_inspect,
        NodeContainerSummary {
          node: node.name.clone(),
          ip_address: node.ip_address.clone(),
          container,
        },
      ))
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(ContainerInspectResponse, NodeContainerSummary), _>>>()
    .await.into_iter().collect::<Result<Vec<(ContainerInspectResponse, NodeContainerSummary)>, _>>()?;
  let mut containers = Vec::new();
  for (container_inspect, node_container_summary) in container_inspects {
    let state = container_inspect.state.unwrap_or_default();
    if let Some(exit_code) = state.exit_code {
      if exit_code == 0 {
        instance_success += 1;
      }
    }
    containers.push(node_container_summary);
  }
  let job_inspect = JobInspect {
    name: job.name,
    created_at: job.created_at,
    updated_at: job.updated_at,
    secrets: job.secrets,
    metadata: job.metadata,
    containers: job.containers,
    instance_total: containers.len(),
    instance_success,
    instances: containers,
  };
  Ok(job_inspect)
}

/// ## Logs by name
///
/// Get the logs of a job by name
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [Stream](StreamExt) of [JobLogOutput](JobLogOutput)
///   * [Err](Err) - [Http error](HttpError) Something went wrong
///
pub async fn logs_by_name(
  name: &str,
  state: &DaemonState,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
  let _ = repositories::job::find_by_name(name, &state.pool).await?;
  let instances = list_instances(name, &state.docker_api).await?;
  let futures = instances
    .into_iter()
    .map(|instance| {
      state
        .docker_api
        .logs(
          &instance.id.unwrap_or_default(),
          Some(LogsOptions::<String> {
            stdout: true,
            ..Default::default()
          }),
        )
        .map(move |elem| match elem {
          Err(err) => Err(err),
          Ok(elem) => Ok(JobLogOutput {
            container_name: instance
              .names
              .clone()
              .unwrap_or_default()
              .join("")
              .replace('/', ""),
            log: elem.into(),
          }),
        })
    })
    .collect::<Vec<_>>();
  let stream = select_all(futures).into_stream();
  Ok(transform_stream::<JobLogOutput, JobLogOutput>(stream))
}

/// ## Wait
///
/// Wait a job to finish
/// And create his instances (containers).
///
/// ## Arguments
///
/// * [key](str) - The job key
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Stream) - The stream of wait
///   * [Err](HttpError) - The job cannot be waited
///
pub async fn wait(
  name: &str,
  wait_options: WaitContainerOptions<WaitCondition>,
  state: &DaemonState,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  let docker_api = state.docker_api.clone();
  let containers = list_instances(&job.name, &docker_api).await?;
  let mut streams = Vec::new();
  for container in containers {
    let id = container.id.unwrap_or_default();
    let options = Some(wait_options.clone());
    let stream =
      docker_api
        .wait_container(&id, options)
        .map(move |wait_result| match wait_result {
          Err(err) => Err(err),
          Ok(wait_response) => {
            Ok(JobWaitResponse::from_container_wait_response(
              wait_response,
              id.to_owned(),
            ))
          }
        });
    streams.push(stream);
  }
  let stream = select_all(streams).into_stream();
  Ok(transform_stream::<JobWaitResponse, JobWaitResponse>(stream))
}
