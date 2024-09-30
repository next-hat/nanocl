use bollard_next::container::{
  RemoveContainerOptions, RenameContainerOptions, StopContainerOptions,
};
use futures::stream::FuturesUnordered;
use futures_util::StreamExt;
use ntex::rt;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::{
  cargo_spec::ReplicationMode,
  process::ProcessKind,
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{CargoDb, ObjPsStatusDb, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

use super::generic::*;

// impl ObjTask for CargoDb {}

impl ObjTaskStart for CargoDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let cargo =
        CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, &state.inner.pool)
          .await?;
      // TODO: FIND BEST NODES TO RUN WORKLOAD
      // let nodes =
      //   MetricDb::find_best_nodes(90.0, 90.0, 100, &state.inner.pool).await?;
      // log::debug!("BEST NODES FOR CARGO {key}: {nodes:?}");
      if processes.is_empty() {
        let number = match &cargo.spec.replication {
          Some(ReplicationMode::Static(replication)) => replication.number,
          _ => 1,
        };
        utils::container::cargo::create(&cargo, number, &state).await?;
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
      let processes =
        ProcessDb::read_by_kind_key(&key, &state.inner.pool).await?;
      for process in processes {
        let _ = state
          .inner
          .docker_api
          .stop_container(&process.key, None::<StopContainerOptions>)
          .await;
        let _ = state
          .inner
          .docker_api
          .remove_container(&process.key, None::<RemoveContainerOptions>)
          .await;
      }
      let cargo =
        CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      CargoDb::clear_by_pk(&key, &state.inner.pool).await?;
      log::debug!("emitting cargo destroy");
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Destroy)
        .await;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskUpdate for CargoDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let cargo =
        CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&key, &state.inner.pool).await?;
      // rename old instances to flag them for deletion
      processes
        .iter()
        .map(|process| {
          let docker_api = state.inner.docker_api.clone();
          async move {
            if process
              .data
              .state
              .clone()
              .unwrap_or_default()
              .restarting
              .unwrap_or_default()
            {
              docker_api
                .stop_container(&process.name, None::<StopContainerOptions>)
                .await?;
            }
            let new_name = format!("tmp-{}", process.name);
            docker_api
              .rename_container(
                &process.key,
                RenameContainerOptions { name: &new_name },
              )
              .await?;
            Ok::<_, HttpError>(())
          }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<HttpResult<Vec<_>>>()?;
      let number = match &cargo.spec.replication {
        Some(ReplicationMode::Static(replication)) => replication.number,
        _ => 1,
      };
      // Create instance with the new spec
      let new_instances =
        match utils::container::cargo::create(&cargo, number, &state).await {
          Err(err) => {
            log::error!(
              "Unable to create cargo instance {} : {err}",
              cargo.spec.cargo_key
            );
            return Err(err.into());
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
          let _ = utils::container::delete_instances(
            &new_instances
              .iter()
              .map(|p| p.key.clone())
              .collect::<Vec<_>>(),
            &state_ptr_ptr,
          )
          .await;
          let res = processes
            .iter()
            .map(|process| {
              let docker_api = state_ptr_ptr.inner.docker_api.clone();
              async move {
                docker_api
                  .rename_container(
                    &process.key,
                    RenameContainerOptions {
                      name: &process.name,
                    },
                  )
                  .await?;
                Ok::<_, HttpError>(())
              }
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<HttpResult<Vec<_>>>();
          if let Err(err) = res {
            log::error!("Unable to rename containers back: {err}");
          }
        }
        Ok(_) => {
          log::debug!("cargo instance {} started", cargo.spec.cargo_key);
          // Delete old containers
          let state_ptr_ptr = state.clone();
          rt::spawn(async move {
            ntex::time::sleep(std::time::Duration::from_secs(4)).await;
            let _ = utils::container::delete_instances(
              &processes.iter().map(|p| p.key.clone()).collect::<Vec<_>>(),
              &state_ptr_ptr,
            )
            .await;
          });
        }
      }
      ObjPsStatusDb::update_actual_status(
        &key,
        &ObjPsStatusKind::Start,
        &state.inner.pool,
      )
      .await?;
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Start)
        .await;
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
