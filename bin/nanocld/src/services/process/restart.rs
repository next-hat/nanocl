use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::{
  process::ProcessKind,
  system::{
    EventActorKind, NativeEventAction, ObjPsHealthStatusKind, ObjPsStatusKind,
  },
};

use crate::{
  models::{CargoDb, ObjPsStatusDb, SystemState},
  repositories::generic::*,
  tasks::generic::{ObjTask, ObjTaskFuture},
  utils,
};

async fn run_cargo_restart(
  cargo: &nanocl_stubs::cargo::Cargo,
  state: &SystemState,
) -> IoResult<()> {
  let key = &cargo.spec.cargo_key;
  let mutex = CargoDb::create_mutex(key, "restart", state).await?;
  let result = utils::container::cargo::restart(key, state).await;
  let cleanup = CargoDb::delete_mutex(&mutex.key, state).await;
  match (result, cleanup) {
    (Err(error), Err(cleanup_error)) => {
      log::error!(
        "Unable to release Cargo restart mutex {} after {error}: {cleanup_error}",
        mutex.key
      );
      Err(error)
    }
    (Err(error), _) => Err(error),
    (Ok(()), Err(error)) => Err(error),
    (Ok(()), Ok(())) => {
      state
        .emit_normal_native_action_sync(cargo, NativeEventAction::Restart)
        .await;
      Ok(())
    }
  }
}

async fn enqueue_cargo_restart(
  key: &str,
  state: &SystemState,
) -> HttpResult<()> {
  let task_key = format!("{}@{key}", EventActorKind::Cargo);
  state.inner.task_manager.wait_task(&task_key).await;
  let cargo = CargoDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let task_key_ptr = task_key.clone();
  let task_cargo = cargo.clone();
  let task_state = state.clone();
  let task: ObjTaskFuture =
    Box::pin(async move { run_cargo_restart(&task_cargo, &task_state).await });
  let error_state = state.clone();
  state
    .inner
    .task_manager
    .add_task(
      &task_key_ptr,
      NativeEventAction::Restart,
      task,
      move |error| async move {
        let key = cargo.spec.cargo_key.clone();
        if ObjPsStatusDb::read_by_pk(&key, &error_state.inner.pool)
          .await
          .is_ok_and(|status| {
            status.actual == ObjPsStatusKind::Starting.to_string()
              && status.wanted == ObjPsStatusKind::Start.to_string()
          })
        {
          ObjPsStatusDb::update_health_status(
            &key,
            &ObjPsHealthStatusKind::Unhealthy,
            &error_state.inner.pool,
          )
          .await?;
          ObjPsStatusDb::update_actual_status(
            &key,
            &ObjPsStatusKind::Fail,
            &error_state.inner.pool,
          )
          .await?;
        }
        error_state.emit_error_native_action(
          &cargo,
          NativeEventAction::Restart,
          Some(error.to_string()),
        );
        Ok::<_, IoError>(())
      },
    )
    .await;
  Ok(())
}

/// Restart all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{key}/restart",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
  ),
  responses(
    (status = 202, description = "Process instances restarted"),
  ),
))]
#[web::post("/processes/{kind}/{key}/restart")]
pub async fn restart_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  if kind == ProcessKind::Cargo {
    enqueue_cargo_restart(&key, &state).await?;
  } else {
    utils::container::process::restart_instances(&key, &kind, &state).await?;
  }
  Ok(web::HttpResponse::Accepted().finish())
}

/// Restart a single process by its name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{name}/restart",
  params(
    ("name" = String, Path, description = "Name or id of the container", example = "system.nstore.c"),
  ),
  responses(
    (status = 202, description = "Process restarted"),
  ),
))]
#[web::post("/processes/{name}/restart")]
pub async fn restart_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  state
    .inner
    .docker_api
    .restart_container(&name, None)
    .await?;
  Ok(web::HttpResponse::Accepted().finish())
}
