use futures_util::StreamExt;

use bollard_next::container::{StartContainerOptions, WaitContainerOptions};

use nanocl_error::{io::IoError, http::HttpError};

use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{JobDb, ObjPsStatusDb, ProcessDb, SystemState},
};

use super::generic::*;

// impl ObjTask for JobDb {}

impl ObjTaskStart for JobDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let job = JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let mut processes =
        ProcessDb::read_by_kind_key(&job.name, &state.inner.pool).await?;
      if processes.is_empty() {
        processes =
          utils::container::create_job_instances(&job, &state).await?;
      }
      ObjPsStatusDb::update_actual_status(
        &key,
        &ObjPsStatusKind::Start,
        &state.inner.pool,
      )
      .await?;
      state.emit_normal_native_action(&job, NativeEventAction::Start);
      for process in processes {
        // We currently run a sequential order so we wait for the container to finish to start the next one.
        let mut stream = state.inner.docker_api.wait_container(
          &process.key,
          Some(WaitContainerOptions {
            condition: "next-exit",
          }),
        );
        let _ = state
          .inner
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
    let state = state.clone();
    Box::pin(async move {
      let job = JobDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&key, &state.inner.pool).await?;
      utils::container::delete_instances(
        &processes
          .iter()
          .map(|p| p.key.clone())
          .collect::<Vec<String>>(),
        &state,
      )
      .await?;
      JobDb::clear_by_pk(&job.name, &state.inner.pool).await?;
      if job.schedule.is_some() {
        utils::cron::remove_cron_rule(&job, &state).await?;
      }
      state.emit_normal_native_action(&job, NativeEventAction::Destroy);
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for JobDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::stop_instances(&key, &ProcessKind::Job, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}
