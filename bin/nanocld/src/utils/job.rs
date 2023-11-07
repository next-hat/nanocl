use bollard_next::container::CreateContainerOptions;

use nanocl_error::http::HttpError;
use nanocl_stubs::job::{Job, JobPartial};

use crate::repositories;
use crate::models::DaemonState;

async fn run_job(job: &Job, state: &DaemonState) -> Result<(), HttpError> {
  for container in &job.containers {
    state
      .docker_api
      .create_container(
        None::<CreateContainerOptions<String>>,
        container.clone(),
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
  Ok(job)
}
