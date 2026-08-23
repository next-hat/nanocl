use ntex::web;

use bollard_next::container::StopContainerOptions;
use nanocl_error::http::{HttpError, HttpResult};

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
#[web::post("/processes/{kind}/{key}/stop")]
pub async fn stop_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  utils::container::generic::emit_stopping(&key, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
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
#[web::post("/processes/{name}/stop")]
pub async fn stop_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  state
    .inner
    .docker_api
    .stop_container(&name, None::<StopContainerOptions>)
    .await?;
  Ok(web::HttpResponse::Accepted().finish())
}
