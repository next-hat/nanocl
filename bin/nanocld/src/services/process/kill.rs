use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::ProcessKillOptions;

use crate::models::{SystemState, VmLifecycleGuard};

async fn lock_vm_signal(
  runtime: &bollard_next::service::ContainerInspectResponse,
  signal: &str,
) -> HttpResult<Option<VmLifecycleGuard>> {
  let Some(key) = super::managed_vm_runtime_key(runtime, false)? else {
    return Ok(None);
  };
  // An explicit hard kill is the operator's escape hatch when a guest ignores
  // ACPI. It targets the inspected immutable ID, even while shutdown waits.
  if signal.eq_ignore_ascii_case("SIGKILL")
    || signal.eq_ignore_ascii_case("KILL")
    || signal == "9"
  {
    return Ok(None);
  }
  Ok(Some(
    crate::utils::container::vm::lifecycle_lock(&key)
      .try_lock_owned()
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?,
  ))
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
  super::reject_job_container_action(&runtime)?;
  let id = runtime
    .id
    .as_deref()
    .ok_or_else(|| HttpError::conflict("Process has no container identity"))?;
  let _guard = lock_vm_signal(&runtime, &payload.signal).await?;
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

  #[ntex::test]
  async fn vm_signals_coordinate_but_hard_kill_remains_available_during_shutdown()
   {
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
    let guard = super::lock_vm_signal(&runtime, "SIGTERM")
      .await
      .unwrap()
      .unwrap();
    for signal in ["SIGTERM", "sigterm", "term", "15"] {
      let error = match super::lock_vm_signal(&runtime, signal).await {
        Err(error) => error,
        Ok(_) => panic!("concurrent VM signal must reject mutation admission"),
      };
      assert_eq!(error.status, ntex::http::StatusCode::CONFLICT);
    }
    for signal in ["SIGKILL", "KILL", "9", "sigkill", "kill", "SigKill", "Kill"]
    {
      assert!(
        super::lock_vm_signal(&runtime, signal)
          .await
          .unwrap()
          .is_none()
      );
    }
    assert!(
      crate::utils::container::vm::lifecycle_lock("global.signal-lock")
        .try_lock_owned()
        .await
        .is_err()
    );
    drop(guard);
    assert!(
      super::lock_vm_signal(&runtime, "SIGTERM")
        .await
        .unwrap()
        .is_some()
    );

    runtime.name = Some("/global.other.v".into());
    assert!(super::lock_vm_signal(&runtime, "SIGKILL").await.is_err());
    runtime.config.as_mut().unwrap().labels = Some(HashMap::new());
    assert!(
      super::lock_vm_signal(&runtime, "SIGTERM")
        .await
        .unwrap()
        .is_none()
    );
    runtime.config.as_mut().unwrap().labels = Some(HashMap::from([(
      "io.nanocl.init-v".into(),
      "global.signal-lock".into(),
    )]));
    assert!(
      super::lock_vm_signal(&runtime, "SIGTERM")
        .await
        .unwrap()
        .is_none()
    );
  }
}
