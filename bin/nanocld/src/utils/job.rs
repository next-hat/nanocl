use bollard_next::container::{CreateContainerOptions, StartContainerOptions};

use nanocl_error::http::HttpError;
use nanocl_stubs::job::{Job, JobPartial};

use crate::repositories;
use crate::models::DaemonState;

async fn run_job(job: &Job, state: &DaemonState) -> Result<(), HttpError> {
  let containers = job.containers.clone();
  for mut container in containers {
    let mut labels = container.labels.clone().unwrap_or_default();
    labels.insert("job_name".to_owned(), job.name.to_owned());
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
