use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::ProcessKillOptions;

use crate::models::SystemState;

fn lock_vm_signal(
  runtime: &bollard_next::service::ContainerInspectResponse,
) -> HttpResult<Option<tokio::sync::OwnedMutexGuard<()>>> {
  let Some(key) = super::managed_vm_runtime_key(runtime, false)? else {
    return Ok(None);
  };
  crate::utils::container::vm::lifecycle_lock(&key)
    .try_lock_owned()
    .map(Some)
    .map_err(|_| HttpError::conflict("A VM lifecycle operation is active"))
}

/// Send a signal to a single process by its name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = ProcessKillOptions,
  path = "/processes/{name}/kill",
  params(
    ("name" = String, Path, description = "Name or id of the container", example = "system.nstore.c"),
  ),
  responses(
    (status = 200, description = "Process killed"),
  ),
))]
pub async fn kill_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ProcessKillOptions>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let runtime = state
    .inner
    .docker_api
    .inspect_container(&name, None)
    .await?;
  let id = runtime
    .id
    .as_deref()
    .ok_or_else(|| HttpError::conflict("Process has no container identity"))?;
  let _guard = lock_vm_signal(&runtime)?;
  state
    .inner
    .docker_api
    // Pin the inspected container: a concurrent replacement must not redirect
    // this signal to a different runtime that reused the same name.
    .kill_container(id, Some(payload.into_inner().into()))
    .await?;
  Ok(web::HttpResponse::Ok().build())
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::service::{ContainerConfig, ContainerInspectResponse};

  #[test]
  fn vm_signals_share_lifecycle_lock_and_leave_other_processes_unchanged() {
    let mut runtime = ContainerInspectResponse {
      name: Some("/global.signal-lock.v".into()),
      config: Some(ContainerConfig {
        labels: Some(HashMap::from([(
          "io.nanocl.v".into(),
          "global.signal-lock".into(),
        )])),
        ..Default::default()
      }),
      ..Default::default()
    };
    let guard = super::lock_vm_signal(&runtime).unwrap().unwrap();
    assert!(super::lock_vm_signal(&runtime).is_err());
    assert!(
      crate::utils::container::vm::lifecycle_lock("global.signal-lock")
        .try_lock_owned()
        .is_err()
    );
    drop(guard);
    assert!(super::lock_vm_signal(&runtime).unwrap().is_some());

    runtime.name = Some("/global.other.v".into());
    assert!(super::lock_vm_signal(&runtime).is_err());
    runtime.config.as_mut().unwrap().labels = Some(HashMap::new());
    assert!(super::lock_vm_signal(&runtime).unwrap().is_none());
    runtime.config.as_mut().unwrap().labels = Some(HashMap::from([(
      "io.nanocl.init-v".into(),
      "global.signal-lock".into(),
    )]));
    assert!(super::lock_vm_signal(&runtime).unwrap().is_none());
  }
}
