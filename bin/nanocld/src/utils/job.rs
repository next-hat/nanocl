use std::collections::HashMap;

use ntex::web;
use ntex::util::Bytes;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use futures_util::{StreamExt, TryStreamExt};
use futures_util::stream::{FuturesUnordered, select_all, FuturesOrdered};
use bollard_next::service::{ContainerSummary, ContainerWaitExitError};
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  RemoveContainerOptions, LogsOptions, WaitContainerOptions,
};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::job::{
  Job, JobPartial, JobInspect, JobLogOutput, JobWaitResponse, WaitCondition,
  JobSummary,
};

use crate::{version, repositories};
use crate::models::{DaemonState, JobUpdateDb};

use super::stream::transform_stream;

/// ## Count instances
///
/// Count the number of instances (containers) of a job
///
/// ## Arguments
///
/// * [instances](Vec) - Instances of [ContainerInspectResponse](ContainerInspectResponse) and [NodeContainerSummary](NodeContainerSummary)
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// The tuple of the number of instances
///   * [usize] - The total number of instances
///   * [usize] - The number of failed instances
///   * [usize] - The number of success instances
///   * [usize] - The number of running instances
///
fn count_instances(
  instances: &[NodeContainerSummary],
) -> (usize, usize, usize, usize) {
  let mut instance_failed = 0;
  let mut instance_success = 0;
  let mut instance_running = 0;
  for instance in instances {
    let container = &instance.container;
    let state = container.state.clone().unwrap_or_default();
    if state.running.unwrap_or_default() {
      instance_running += 1;
      continue;
    }
    if let Some(exit_code) = state.exit_code {
      if exit_code == 0 {
        instance_success += 1;
      } else {
        instance_failed += 1;
      }
    }
    if let Some(error) = state.error {
      if !error.is_empty() {
        instance_failed += 1;
      }
    }
  }
  (
    instances.len(),
    instance_failed,
    instance_success,
    instance_running,
  )
}

/// ## Format cron job command
///
/// Format the cron job command to start a job at a given time
///
/// ## Arguments
///
/// * [job](Job) - The job
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [String](String) - The cron job command
///
fn format_cron_job_command(job: &Job, state: &DaemonState) -> String {
  let host = state
    .config
    .hosts
    .get(0)
    .cloned()
    .unwrap_or("unix:///run/nanocl/nanocl.sock".to_owned())
    .replace("unix://", "");
  format!(
    "curl -X POST --unix {host} http://localhost/v{}/jobs/{}/start",
    version::VERSION,
    &job.name
  )
}

/// ## Exec crontab
///
/// Execute the crontab command to update the cron jobs
///
async fn exec_crontab() -> IoResult<()> {
  web::block(|| {
    std::process::Command::new("crontab")
      .arg("/tmp/crontab")
      .output()
      .map_err(|err| err.map_err_context(|| "Cron job"))?;
    Ok::<_, IoError>(())
  })
  .await?;
  Ok(())
}

/// ## Add cron rule
///
/// Add a cron rule to the crontab to start a job at a given time
///
/// ## Arguments
///
/// * [item](Job) - The job
/// * [schedule](str) - The schedule  policy
/// * [state](DaemonState) - The daemon state
///
async fn add_cron_rule(
  item: &Job,
  schedule: &str,
  state: &DaemonState,
) -> IoResult<()> {
  let cmd = format_cron_job_command(item, state);
  let cron_rule = format!("{} {cmd}", schedule);
  log::debug!("Creating cron rule: {cron_rule}");
  fs::copy("/var/spool/cron/crontabs/root", "/tmp/crontab")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  let mut file = fs::OpenOptions::new()
    .write(true)
    .append(true)
    .open("/tmp/crontab")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  file
    .write_all(format!("{cron_rule}\n").as_bytes())
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  exec_crontab().await?;
  Ok(())
}

/// ## Remove cron rule
///
/// Remove a cron rule from the crontab for the given job
///
/// ## Arguments
///
/// * [item](Job) - The job
/// * [state](DaemonState) - The daemon state
///
async fn remove_cron_rule(item: &Job, state: &DaemonState) -> IoResult<()> {
  let mut content = fs::read_to_string("/var/spool/cron/crontabs/root")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  let cmd = format_cron_job_command(item, state);
  log::debug!("Removing cron rule: {cmd}");
  content = content
    .lines()
    .filter(|line| !line.contains(&cmd))
    .collect::<Vec<_>>()
    .join("\n");
  fs::write("/tmp/crontab", format!("{content}\n"))
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  exec_crontab().await?;
  Ok(())
}

/// ## Inspect instances
///
/// Return detailed informations about each instances of a job
///
/// ## Arguments
///
/// [name](str) The job name
/// [state](DaemonState) The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [NodeContainerSummary](NodeContainerSummary)
///
async fn inspect_instances(
  name: &str,
  state: &DaemonState,
) -> HttpResult<Vec<NodeContainerSummary>> {
  // Convert into a hashmap for faster lookup
  let nodes = repositories::node::list(&state.pool).await?;
  let nodes = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<String, _>>();
  repositories::container_instance::list_for_kind("Job", name, &state.pool)
    .await?
    .into_iter()
    .map(|instance| {
      Ok::<_, HttpError>(NodeContainerSummary {
        node: instance.node_id.clone(),
        ip_address: match nodes.get(&instance.node_id) {
          Some(node) => node.ip_address.clone(),
          None => "Unknow".to_owned(),
        },
        container: instance.data,
      })
    })
    .collect::<Result<Vec<NodeContainerSummary>, _>>()
}

/// ## List instances
///
/// List the job instances (containers) based on the job name
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [docker_api](bollard_next::Docker) - The docker api
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [ContainerSummary](ContainerSummary)
///
pub(crate) async fn list_instances(
  name: &str,
  docker_api: &bollard_next::Docker,
) -> HttpResult<Vec<ContainerSummary>> {
  let label = format!("io.nanocl.j={name}");
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

/// ## Create
///
/// Create a job and run it
///
/// ## Arguments
///
/// * [item](JobPartial) - The job partial
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) the created [job](Job)
///
pub(crate) async fn create(
  item: &JobPartial,
  state: &DaemonState,
) -> HttpResult<Job> {
  let job = repositories::job::create(item, &state.pool).await?;
  job
    .containers
    .iter()
    .map(|container| {
      let job_name = job.name.clone();
      async move {
        let mut container = container.clone();
        let mut labels = container.labels.clone().unwrap_or_default();
        labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
        labels.insert("io.nanocl.kind".to_owned(), "Job".to_owned());
        labels.insert("io.nanocl.j".to_owned(), job_name.clone());
        container.labels = Some(labels);
        state
          .docker_api
          .create_container(
            None::<CreateContainerOptions<String>>,
            container.clone(),
          )
          .await?;
        Ok::<_, HttpError>(())
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  if let Some(schedule) = &job.schedule {
    add_cron_rule(&job, schedule, state).await?;
  }
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
pub(crate) async fn start_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  repositories::job::find_by_name(name, &state.pool).await?;
  let containers = inspect_instances(name, state).await?;
  containers
    .into_iter()
    .map(|inspect| async {
      if inspect
        .container
        .state
        .unwrap_or_default()
        .running
        .unwrap_or_default()
      {
        return Ok(());
      }
      state
        .docker_api
        .start_container(
          &inspect.container.id.unwrap_or_default(),
          None::<StartContainerOptions<String>>,
        )
        .await?;
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesOrdered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  repositories::job::update_by_name(
    name,
    &JobUpdateDb {
      updated_at: Some(chrono::Utc::now().naive_utc()),
    },
    &state.pool,
  )
  .await?;
  Ok(())
}

/// ## List
///
/// List all jobs
///
/// ## Arguments
///
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [JobSummary](JobSummary)
///
pub(crate) async fn list(state: &DaemonState) -> HttpResult<Vec<JobSummary>> {
  let jobs = repositories::job::list(&state.pool).await?;
  let job_summaries =
    jobs
      .iter()
      .map(|job| async {
        let instances = inspect_instances(&job.name, state).await?;
        let (
          instance_total,
          instance_failed,
          instance_success,
          instance_running,
        ) = count_instances(&instances);
        Ok::<_, HttpError>(JobSummary {
          name: job.name.clone(),
          created_at: job.created_at,
          updated_at: job.updated_at,
          spec: job.clone(),
          instance_total,
          instance_success,
          instance_running,
          instance_failed,
        })
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<JobSummary, HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<JobSummary>, HttpError>>()?;
  Ok(job_summaries)
}

/// ## Delete by name
///
/// Delete a job by name with his given instances (containers).
///
/// ## Arguments
///
/// * [name](str) - The job name
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
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
  if job.schedule.is_some() {
    remove_cron_rule(&job, state).await?;
  }
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
/// ## Return
///
/// [HttpResult](HttpResult) containing a [JobInspect](JobInspect)
///
pub(crate) async fn inspect_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<JobInspect> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  let instances = inspect_instances(name, state).await?;
  let (instance_total, instance_failed, instance_success, instance_running) =
    count_instances(&instances);
  let job_inspect = JobInspect {
    name: job.name,
    created_at: job.created_at,
    updated_at: job.updated_at,
    secrets: job.secrets,
    metadata: job.metadata,
    schedule: job.schedule,
    containers: job.containers,
    instance_total,
    instance_success,
    instance_running,
    instance_failed,
    instances,
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
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Stream](StreamExt) of [JobLogOutput](JobLogOutput)
///
pub(crate) async fn logs_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = Result<Bytes, HttpError>>> {
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
/// * [name](str) - The job name
/// * [wait_options](WaitContainerOptions) - The wait options
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Stream](StreamExt) of [JobWaitResponse](JobWaitResponse)
///
pub(crate) async fn wait(
  name: &str,
  wait_options: WaitContainerOptions<WaitCondition>,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = Result<Bytes, HttpError>>> {
  let job = repositories::job::find_by_name(name, &state.pool).await?;
  let docker_api = state.docker_api.clone();
  let containers = list_instances(&job.name, &docker_api).await?;
  let mut streams = Vec::new();
  for container in containers {
    let id = container.id.unwrap_or_default();
    let options = Some(wait_options.clone());
    let container_name = container
      .names
      .clone()
      .unwrap_or_default()
      .join("")
      .replace('/', "");
    let stream =
      docker_api
        .wait_container(&id, options)
        .map(move |wait_result| match wait_result {
          Err(err) => {
            if let bollard_next::errors::Error::DockerContainerWaitError {
              error,
              code,
            } = &err
            {
              return Ok(JobWaitResponse {
                container_name: container_name.clone(),
                status_code: *code,
                error: Some(ContainerWaitExitError {
                  message: Some(error.to_owned()),
                }),
              });
            }
            Err(err)
          }
          Ok(wait_response) => {
            Ok(JobWaitResponse::from_container_wait_response(
              wait_response,
              container_name.clone(),
            ))
          }
        });
    streams.push(stream);
  }
  let stream = select_all(streams).into_stream();
  Ok(transform_stream::<JobWaitResponse, JobWaitResponse>(stream))
}
