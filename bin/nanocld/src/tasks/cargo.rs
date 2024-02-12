use ntex::rt;
use bollard_next::container::{RemoveContainerOptions, StartContainerOptions};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{
    CargoDb, ObjPsStatusDb, ObjPsStatusUpdate, ObjTask, ProcessDb, SystemState,
  },
};

use super::generic::*;

impl ObjTaskStart for CargoDb {
  async fn create_start_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state = state.clone();
    let task = ObjTask::new(NativeEventAction::Starting, async move {
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      let mut processes =
        ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.pool).await?;
      if processes.is_empty() {
        processes = utils::container::create_cargo(&cargo, 1, &state).await?;
      }
      for process in processes {
        let _ = state
          .docker_api
          .start_container(&process.key, None::<StartContainerOptions<String>>)
          .await;
      }
      let cur_status =
        ObjPsStatusDb::read_by_pk(&cargo.spec.cargo_key, &state.pool).await?;
      let new_status = ObjPsStatusUpdate {
        wanted: Some(ObjPsStatusKind::Start.to_string()),
        prev_wanted: Some(cur_status.wanted),
        actual: Some(ObjPsStatusKind::Start.to_string()),
        prev_actual: Some(cur_status.actual),
      };
      ObjPsStatusDb::update_pk(&cargo.spec.cargo_key, new_status, &state.pool)
        .await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Start);
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}

impl ObjTaskDelete for CargoDb {
  async fn create_delete_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state = state.clone();
    log::debug!("handling delete event for cargo {key}");
    let task = ObjTask::new(NativeEventAction::Destroying, async move {
      let processes = ProcessDb::read_by_kind_key(&key, &state.pool).await?;
      for process in processes {
        let _ = state
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
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      CargoDb::clear_by_pk(&key, &state.pool).await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Destroy);
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}

impl ObjTaskUpdate for CargoDb {
  async fn create_update_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state = state.clone();
    let task = ObjTask::new(NativeEventAction::Updating, async move {
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      let processes = ProcessDb::read_by_kind_key(&key, &state.pool).await?;
      // Create instance with the new spec
      let new_instances =
        match utils::container::create_cargo(&cargo, 1, &state).await {
          Err(err) => {
            log::warn!(
              "Unable to create cargo instance {} : {err}",
              cargo.spec.cargo_key
            );
            Vec::default()
          }
          Ok(instances) => instances,
        };
      // start created containers
      match utils::container::start_instances(&key, &ProcessKind::Cargo, &state)
        .await
      {
        Err(err) => {
          log::error!(
            "Unable to start cargo instance {} : {err}",
            cargo.spec.cargo_key
          );
          let state_ptr_ptr = state.clone();
          rt::spawn(async move {
            ntex::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = utils::container::delete_instances(
              &new_instances
                .iter()
                .map(|p| p.key.clone())
                .collect::<Vec<_>>(),
              &state_ptr_ptr,
            )
            .await;
          });
        }
        Ok(_) => {
          // Delete old containers
          utils::container::delete_instances(
            &processes.iter().map(|p| p.key.clone()).collect::<Vec<_>>(),
            &state,
          )
          .await?;
        }
      }
      state.emit_normal_native_action(&cargo, NativeEventAction::Start);
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}

impl ObjTaskStop for CargoDb {
  async fn create_stop_task(
    key: &str,
    state: &SystemState,
  ) -> IoResult<ObjTask> {
    let key = key.to_owned();
    let state = state.clone();
    let task = ObjTask::new(NativeEventAction::Stopping, async move {
      utils::container::stop_instances(&key, &ProcessKind::Cargo, &state)
        .await?;
      let curr_status = ObjPsStatusDb::read_by_pk(&key, &state.pool).await?;
      let new_status = ObjPsStatusUpdate {
        wanted: Some(ObjPsStatusKind::Stop.to_string()),
        prev_wanted: Some(curr_status.wanted),
        actual: Some(ObjPsStatusKind::Stop.to_string()),
        prev_actual: Some(curr_status.actual),
      };
      ObjPsStatusDb::update_pk(&key, new_status, &state.pool).await?;
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Stop);
      Ok::<_, IoError>(())
    });
    Ok(task)
  }
}
