use ntex::web;

use bollard_next::container::StopContainerOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{process::ProcessKind, vm::VmLifecycleOptions};

use crate::{models::SystemState, utils};

/// Stop all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{key}/stop",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
  ),
  responses(
    (status = 202, description = "Process instances stopped"),
  ),
))]
pub async fn stop_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  if kind == ProcessKind::Vm {
    crate::services::vm::console::authorize_request(&req)?;
    utils::container::vm::stop_with_options(
      &key,
      &VmLifecycleOptions::default(),
      state,
    )
    .await
    .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
  } else {
    utils::container::generic::emit_stopping(&key, &kind, &state).await?;
  }
  Ok(web::HttpResponse::Accepted().build())
}

/// Stop a single process by its name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{name}/stop",
  params(
    ("name" = String, Path, description = "Name or id of the container", example = "system.nstore.c"),
  ),
  responses(
    (status = 202, description = "Process stopped"),
  ),
))]
pub async fn stop_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let runtime = state
    .inner
    .docker_api
    .inspect_container(&name, None)
    .await?;
  if let Some(key) = super::managed_vm_runtime_key(&runtime, false)? {
    crate::services::vm::console::authorize_request(&req)?;
    let id = runtime
      .id
      .as_deref()
      .ok_or_else(|| HttpError::conflict("VM runtime has no container ID"))?;
    utils::container::vm::control_process(&key, id, false, state)
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
    return Ok(web::HttpResponse::Accepted().build());
  }
  state
    .inner
    .docker_api
    .stop_container(&name, None::<StopContainerOptions>)
    .await?;
  Ok(web::HttpResponse::Accepted().build())
}
