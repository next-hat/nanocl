use ntex::rt;
use bollard_next::container::RemoveContainerOptions;

use nanocl_error::io::IoError;
use nanocl_stubs::{
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{CargoDb, ObjPsStatusDb, ProcessDb, SystemState},
};

use super::generic::*;

impl ObjTask for CargoDb {}

impl ObjTaskStart for CargoDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let cargo = CargoDb::transform_read_by_pk(&key, &state.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.pool).await?;
      if processes.is_empty() {
        utils::container::create_cargo(&cargo, 1, &state).await?;
      }
      utils::container::start_instances(
        &cargo.spec.cargo_key,
        &ProcessKind::Cargo,
        &state,
      )
      .await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for CargoDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    log::debug!("handling delete event for cargo {key}");
    Box::pin(async move {
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
    })
  }
}

impl ObjTaskUpdate for CargoDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
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
          let state_ptr_ptr = state.clone();
          let _ = utils::container::delete_instances(
            &processes.iter().map(|p| p.key.clone()).collect::<Vec<_>>(),
            &state_ptr_ptr,
          )
          .await;
        }
      }
      ObjPsStatusDb::update_actual_status(
        &key,
        &ObjPsStatusKind::Start,
        &state.pool,
      )
      .await?;
      state.emit_normal_native_action(&cargo, NativeEventAction::Start);
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for CargoDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::stop_instances(&key, &ProcessKind::Cargo, &state)
        .await?;
      Ok::<_, IoError>(())
    })
  }
}
