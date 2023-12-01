use std::collections::HashMap;

use nanocl_stubs::generic::GenericFilter;
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

use crate::version;
use crate::models::{
  DaemonState, JobUpdateDb, ContainerDb, JobDb, Repository, FromSpec, NodeDb,
};

use super::stream::transform_stream;

/// Count the number of instances (containers) of a job
pub(crate) fn count_instances(
  instances: &[NodeContainerSummary],
) -> (usize, usize, usize, usize) {
  let mut instance_failed = 0;
  let mut instance_success = 0;
  let mut instance_running = 0;
  for instance in instances {
    let container = &instance.container;
    let state = container.state.clone().unwrap_or_default();
    if state.restarting.unwrap_or_default() {
      instance_failed += 1;
      continue;
    }
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

/// Format the cron job command to start a job at a given time
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

/// Execute the crontab command to update the cron jobs
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

/// Add a cron rule to the crontab to start a job at a given time
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

/// Remove a cron rule from the crontab for the given job
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

/// Return detailed informations about each instances of a job
pub(crate) async fn inspect_instances(
  name: &str,
  state: &DaemonState,
) -> HttpResult<Vec<NodeContainerSummary>> {
  // Convert into a hashmap for faster lookup
  let nodes = NodeDb::find(&GenericFilter::default(), &state.pool).await??;
  let nodes = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<String, _>>();
  ContainerDb::find_by_kind_id(name, &state.pool)
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

/// List the job instances (containers) based on the job name
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

/// Create a job and run it
pub(crate) async fn create(
  item: &JobPartial,
  state: &DaemonState,
) -> HttpResult<Job> {
  let db_model =
    JobDb::try_from_spec_partial(&item.name, crate::version::VERSION, item)?;
  let job = JobDb::create(db_model, &state.pool).await??.to_spec(item);
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

/// Start a job by name
pub(crate) async fn start_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  JobDb::find_by_pk(name, &state.pool).await??;
  let containers = list_instances(name, &state.docker_api).await?;
  containers
    .into_iter()
    .map(|inspect| async {
      if inspect.state.unwrap_or_default() == "running" {
        return Ok(());
      }
      state
        .docker_api
        .start_container(
          &inspect.id.unwrap_or_default(),
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
  JobDb::update_by_pk(
    name,
    JobUpdateDb {
      updated_at: Some(chrono::Utc::now().naive_utc()),
    },
    &state.pool,
  )
  .await??;
  Ok(())
}

/// List all jobs
pub(crate) async fn list(state: &DaemonState) -> HttpResult<Vec<JobSummary>> {
  let jobs = JobDb::find(&GenericFilter::default(), &state.pool).await??;
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
          instance_total,
          instance_success,
          instance_running,
          instance_failed,
          spec: job.clone(),
        })
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<JobSummary, HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<JobSummary>, HttpError>>()?;
  Ok(job_summaries)
}

/// Delete a job by name with his given instances (containers).
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let job = JobDb::find_by_pk(name, &state.pool).await??.try_to_spec()?;
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
  JobDb::delete_by_pk(&job.name, &state.pool).await??;
  if job.schedule.is_some() {
    remove_cron_rule(&job, state).await?;
  }
  Ok(())
}

/// Inspect a job by name and return a detailed view of the job
pub(crate) async fn inspect_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<JobInspect> {
  let job = JobDb::find_by_pk(name, &state.pool).await??.try_to_spec()?;
  let instances = inspect_instances(name, state).await?;
  let (instance_total, instance_failed, instance_success, instance_running) =
    count_instances(&instances);
  let job_inspect = JobInspect {
    spec: job,
    instance_total,
    instance_success,
    instance_running,
    instance_failed,
    instances,
  };
  Ok(job_inspect)
}

/// Get the logs of a job by name
pub(crate) async fn logs_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = Result<Bytes, HttpError>>> {
  JobDb::find_by_pk(name, &state.pool).await??;
  let instances = list_instances(name, &state.docker_api).await?;
  log::debug!("Instances: {instances:#?}");
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
          Ok(elem) => {
            log::debug!("{:#?} {elem}", &instance.names);
            Ok(JobLogOutput {
              container_name: instance
                .names
                .clone()
                .unwrap_or_default()
                .join("")
                .replace('/', ""),
              log: elem.into(),
            })
          }
        })
    })
    .collect::<Vec<_>>();
  let stream = select_all(futures).into_stream();
  Ok(transform_stream::<JobLogOutput, JobLogOutput>(stream))
}

/// Wait a job to finish
/// And create his instances (containers).
pub(crate) async fn wait(
  name: &str,
  wait_options: WaitContainerOptions<WaitCondition>,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = Result<Bytes, HttpError>>> {
  let job = JobDb::find_by_pk(name, &state.pool).await??.try_to_spec()?;
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
