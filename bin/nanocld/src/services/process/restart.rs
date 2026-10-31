use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::ProcessKind;

use crate::{models::SystemState, utils};

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
    (status = 202, description = "Restart accepted", headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID for Cargo; omitted for other kinds. Match terminal event Metadata.Operation to confirm completion.")
    )),
  ),
))]
pub async fn restart_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  let operation = if kind == ProcessKind::Job {
    return Err(HttpError::conflict(
      "Jobs execute immutable runs; stop the active run, then start a new run",
    ));
  } else if kind == ProcessKind::Vm {
    utils::container::vm::enqueue_lifecycle(&key, None, true, state)
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
    None
  } else if kind == ProcessKind::Cargo {
    Some(crate::tasks::cargo::restart_operation(&key, state).await?)
  } else {
    utils::container::process::restart_instances(&key, &kind, &state).await?;
    None
  };
  let mut response = web::HttpResponse::Accepted();
  if let Some(operation) = operation {
    response.set_header("X-Nanocl-Operation", operation.to_string());
  }
  Ok(response.build())
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
pub async fn restart_process(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  let runtime = state
    .inner
    .docker_api
    .inspect_container(&name, None)
    .await?;
  super::reject_job_container_action(&runtime)?;
  if let Some(key) = super::managed_vm_runtime_key(&runtime, false)? {
    let id = runtime
      .id
      .as_deref()
      .ok_or_else(|| HttpError::conflict("VM runtime has no container ID"))?;
    utils::container::vm::enqueue_lifecycle(&key, Some(id), true, state)
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
    return Ok(web::HttpResponse::Accepted().build());
  }
  if let Some(key) = super::stop::managed_cargo_runtime_key(&runtime)? {
    let id = runtime.id.as_deref().ok_or_else(|| {
      HttpError::conflict("Cargo runtime has no container ID")
    })?;
    crate::tasks::cargo::process_action(
      &key,
      id,
      "restart_process",
      None,
      state,
    )
    .await?;
    return Ok(web::HttpResponse::Accepted().build());
  }
  state
    .inner
    .docker_api
    .restart_container(&name, None)
    .await?;
  Ok(web::HttpResponse::Accepted().build())
}
