use ntex::{web, util::Bytes};
use tokio::{fs, io::AsyncWriteExt};
use futures_util::{
  StreamExt, TryStreamExt,
  stream::{FuturesUnordered, select_all},
};

use nanocl_error::{
  io::{FromIo, IoError, IoResult},
  http::{HttpError, HttpResult},
};

use bollard_next::{
  service::ContainerWaitExitError,
  container::{RemoveContainerOptions, WaitContainerOptions},
};
use nanocl_stubs::{
  generic::GenericFilter,
  job::{
    Job, JobPartial, JobInspect, JobWaitResponse, WaitCondition, JobSummary,
  },
};

use crate::{
  version, utils,
  repositories::generic::*,
  models::{DaemonState, ProcessDb, JobDb},
};

use super::stream::transform_stream;

/// Format the cron job command to start a job at a given time
fn format_cron_job_command(job: &Job, state: &DaemonState) -> String {
  let host = state
    .config
    .hosts
    .first()
    .cloned()
    .unwrap_or("unix:///run/nanocl/nanocl.sock".to_owned())
    .replace("unix://", "");
  format!(
    "curl -X POST --unix {host} http://localhost/v{}/processes/job/{}/start",
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

/// Create a job and with it's containers
pub(crate) async fn create(
  item: &JobPartial,
  state: &DaemonState,
) -> HttpResult<Job> {
  let db_model = JobDb::try_from_partial(item)?;
  let job = JobDb::create_from(db_model, &state.pool)
    .await?
    .to_spec(item);
  job
    .containers
    .iter()
    .map(|container| {
      let job_name = job.name.clone();
      async move {
        let mut container = container.clone();
        let mut labels = container.labels.clone().unwrap_or_default();
        labels.insert("io.nanocl.j".to_owned(), job_name.clone());
        container.labels = Some(labels);
        let short_id = utils::key::generate_short_id(6);
        let name = format!("{job_name}-{short_id}.j");
        utils::process::create(&name, "job", &job_name, container, state)
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

/// List all jobs
pub(crate) async fn list(state: &DaemonState) -> HttpResult<Vec<JobSummary>> {
  let jobs = JobDb::read(&GenericFilter::default(), &state.pool).await?;
  let job_summaries =
    jobs
      .iter()
      .map(|job| async {
        let job = job.try_to_spec()?;
        let instances =
          ProcessDb::find_by_kind_key(&job.name, &state.pool).await?;
        let (
          instance_total,
          instance_failed,
          instance_success,
          instance_running,
        ) = utils::process::count_status(&instances);
        Ok::<_, HttpError>(JobSummary {
          instance_total,
          instance_success,
          instance_running,
          instance_failed,
          spec: job.clone(),
        })
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<HttpResult<_>>>()
      .await
      .into_iter()
      .collect::<HttpResult<Vec<_>>>()?;
  Ok(job_summaries)
}

/// Delete a job by name with his given instances (containers).
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let job = JobDb::read_by_pk(name, &state.pool).await?.try_to_spec()?;
  let processes = ProcessDb::find_by_kind_key(name, &state.pool).await?;
  processes
    .into_iter()
    .map(|process| async move {
      utils::process::remove(
        &process.key,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
        state,
      )
      .await
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  JobDb::del_by_pk(&job.name, &state.pool).await?;
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
  let job = JobDb::read_by_pk(name, &state.pool).await?.try_to_spec()?;
  let instances = ProcessDb::find_by_kind_key(name, &state.pool).await?;
  let (instance_total, instance_failed, instance_success, instance_running) =
    utils::process::count_status(&instances);
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

/// Wait a job to finish
pub(crate) async fn wait(
  name: &str,
  wait_options: WaitContainerOptions<WaitCondition>,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = Result<Bytes, HttpError>>> {
  let job = JobDb::read_by_pk(name, &state.pool).await?.try_to_spec()?;
  let docker_api = state.docker_api.clone();
  let processes = ProcessDb::find_by_kind_key(&job.name, &state.pool).await?;
  let mut streams = Vec::new();
  for process in processes {
    let options = Some(wait_options.clone());
    let stream = docker_api.wait_container(&process.key, options).map(
      move |wait_result| match wait_result {
        Err(err) => {
          if let bollard_next::errors::Error::DockerContainerWaitError {
            error,
            code,
          } = &err
          {
            return Ok(JobWaitResponse {
              container_name: process.name.clone(),
              status_code: *code,
              error: Some(ContainerWaitExitError {
                message: Some(error.to_owned()),
              }),
            });
          }
          Err(err)
        }
        Ok(wait_response) => Ok(JobWaitResponse::from_container_wait_response(
          wait_response,
          process.name.clone(),
        )),
      },
    );
    streams.push(stream);
  }
  let stream = select_all(streams).into_stream();
  Ok(transform_stream::<JobWaitResponse, JobWaitResponse>(stream))
}
