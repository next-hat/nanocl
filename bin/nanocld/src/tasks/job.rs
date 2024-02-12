use futures_util::StreamExt;

use bollard_next::container::{
  RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
};
use nanocl_error::{
  io::{IoError, IoResult},
  http::HttpError,
};
use nanocl_stubs::system::NativeEventAction;

use crate::{
  models::{JobDb, ObjTask, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

use super::generic::*;

impl ObjTaskStart for JobDb {
  async fn create_start_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state_ptr = state.clone();
    let task = ObjTask::new(NativeEventAction::Starting, async move {
      let job = JobDb::read_by_pk(&key, &state_ptr.pool)
        .await?
        .try_to_spec()?;
      let mut processes =
        ProcessDb::read_by_kind_key(&job.name, &state_ptr.pool).await?;
      if processes.is_empty() {
        processes = utils::container::create_job(&job, &state_ptr).await?;
      }
      state_ptr.emit_normal_native_action(&job, NativeEventAction::Start);
      for process in processes {
        // We currently run a sequential order so we wait for the container to finish to start the next one.
        let mut stream = state_ptr.docker_api.wait_container(
          &process.key,
          Some(WaitContainerOptions {
            condition: "not-running",
          }),
        );
        let _ = state_ptr
          .docker_api
          .start_container(&process.key, None::<StartContainerOptions<String>>)
          .await;
        while let Some(stream) = stream.next().await {
          let result = stream.map_err(HttpError::internal_server_error)?;
          if result.status_code == 0 {
            break;
          }
        }
      }
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}

impl ObjTaskDelete for JobDb {
  async fn create_delete_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state_ptr = state.clone();
    let task = ObjTask::new(NativeEventAction::Destroying, async move {
      let job = JobDb::read_by_pk(&key, &state_ptr.pool)
        .await?
        .try_to_spec()?;
      let processes =
        ProcessDb::read_by_kind_key(&key, &state_ptr.pool).await?;
      for process in processes {
        let _ = state_ptr
          .docker_api
          .remove_container(
            &process.key,
            Some(RemoveContainerOptions {
              force: true,
              ..Default::default()
            }),
          )
          .await;
      }
      JobDb::clear(&job.name, &state_ptr.pool).await?;
      if job.schedule.is_some() {
        utils::job::remove_cron_rule(&job, &state_ptr).await?;
      }
      state_ptr.emit_normal_native_action(&job, NativeEventAction::Destroy);
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}
