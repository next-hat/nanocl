use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

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
  utils::container::process::restart_instances(&key, &kind, &state).await?;
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
