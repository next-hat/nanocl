use futures_util::{Future, StreamExt};

use bollard_next::container::{
  RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
};
use nanocl_error::{
  io::{IoError, IoResult},
  http::HttpError,
};
use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{
    JobDb, ObjPsStatusDb, ObjPsStatusUpdate, ObjTask, ProcessDb, SystemState,
  },
  repositories::generic::*,
  utils,
};

use super::generic::*;

impl ObjTaskStart for JobDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let job = JobDb::read_by_pk(&key, &state.pool).await?.try_to_spec()?;
      let mut processes =
        ProcessDb::read_by_kind_key(&job.name, &state.pool).await?;
      if processes.is_empty() {
        processes = utils::container::create_job(&job, &state).await?;
      }
      state.emit_normal_native_action(&job, NativeEventAction::Start);
      for process in processes {
        // We currently run a sequential order so we wait for the container to finish to start the next one.
        let mut stream = state.docker_api.wait_container(
          &process.key,
          Some(WaitContainerOptions {
            condition: "not-running",
          }),
        );
        let _ = state
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
    })
  }
}

impl ObjTaskDelete for JobDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state_ptr = state.clone();
    Box::pin(async move {
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
    })
  }
}

impl ObjTaskStop for JobDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state_ptr = state.clone();
    Box::pin(async move {
      utils::container::stop_instances(&key, &ProcessKind::Job, &state_ptr)
        .await?;
      let curr_status =
        ObjPsStatusDb::read_by_pk(&key, &state_ptr.pool).await?;
      let new_status = ObjPsStatusUpdate {
        wanted: Some(ObjPsStatusKind::Stop.to_string()),
        prev_wanted: Some(curr_status.wanted),
        actual: Some(ObjPsStatusKind::Stop.to_string()),
        prev_actual: Some(curr_status.actual),
      };
      ObjPsStatusDb::update_pk(&key, new_status, &state_ptr.pool).await?;
      let job = JobDb::read_by_pk(&key, &state_ptr.pool)
        .await?
        .try_to_spec()?;
      state_ptr.emit_normal_native_action(&job, NativeEventAction::Stop);
      Ok::<_, IoError>(())
    })
  }
}
