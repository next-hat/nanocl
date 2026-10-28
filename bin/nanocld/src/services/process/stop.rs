use ntex::web;

use bollard_next::query_parameters::StopContainerOptions;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::process::ProcessKind;

use crate::{models::SystemState, utils};

/// Recognize Cargo ownership before any raw Docker action. Partial ownership
/// labels must fail closed rather than bypassing the operation admission path.
pub(super) fn managed_cargo_runtime_key(
  runtime: &bollard_next::models::ContainerInspectResponse,
) -> HttpResult<Option<String>> {
  let labels = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  let label = |name: &str| labels.and_then(|labels| labels.get(name));
  let managed = label("io.nanocl.kind").is_some_and(|kind| kind == "cargo")
    || label("io.nanocl.c").is_some()
    || labels.is_some_and(|labels| {
      labels.keys().any(|key| key.starts_with("io.nanocl.cargo."))
    });
  if !managed {
    return Ok(None);
  }
  let key = label("io.nanocl.c").ok_or_else(|| {
    HttpError::conflict("Cargo runtime has no canonical owner key")
  })?;
  let key = key
    .parse::<nanocl_stubs::resource_key::ResourceKey>()
    .map_err(|_| {
      HttpError::conflict("Cargo runtime has an invalid owner key")
    })?;
  Ok(Some(key.to_string()))
}

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
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  if kind == ProcessKind::Job {
    crate::tasks::job::stop(&key, state).await?;
  } else if kind == ProcessKind::Vm {
    utils::container::vm::enqueue_lifecycle(&key, None, false, state)
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
  } else if kind == ProcessKind::Cargo {
    crate::tasks::cargo::stop(&key, state).await?;
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
    utils::container::vm::enqueue_lifecycle(&key, Some(id), false, state)
      .await
      .map_err(crate::services::vm::lifecycle::lifecycle_error)?;
    return Ok(web::HttpResponse::Accepted().build());
  }
  if let Some(key) = managed_cargo_runtime_key(&runtime)? {
    let id = runtime.id.as_deref().ok_or_else(|| {
      HttpError::conflict("Cargo runtime has no container ID")
    })?;
    crate::tasks::cargo::process_action(&key, id, "stop_process", None, state)
      .await?;
    return Ok(web::HttpResponse::Accepted().build());
  }
  state
    .inner
    .docker_api
    .stop_container(&name, None::<StopContainerOptions>)
    .await?;
  Ok(web::HttpResponse::Accepted().build())
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::models::{ContainerConfig, ContainerInspectResponse};

  #[test]
  fn cargo_process_control_never_falls_back_for_partial_owner_labels() {
    let runtime = |labels: &[(&str, &str)]| ContainerInspectResponse {
      config: Some(ContainerConfig {
        labels: Some(
          labels
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>(),
        ),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert_eq!(
      super::managed_cargo_runtime_key(&runtime(&[])).unwrap(),
      None
    );
    assert_eq!(
      super::managed_cargo_runtime_key(&runtime(&[(
        "io.nanocl.c",
        "global.api"
      ),]))
      .unwrap(),
      Some("global.api".into())
    );
    for labels in [
      vec![("io.nanocl.kind", "cargo")],
      vec![("io.nanocl.cargo.incarnation", "old")],
      vec![("io.nanocl.c", "not-a-canonical-key")],
    ] {
      assert_eq!(
        super::managed_cargo_runtime_key(&runtime(&labels))
          .unwrap_err()
          .status,
        ntex::http::StatusCode::CONFLICT,
      );
    }
  }
}
