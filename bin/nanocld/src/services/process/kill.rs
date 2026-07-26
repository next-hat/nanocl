use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::cargo::CargoKillOptions;

use crate::{models::SystemState, utils};

/// Send a signal to all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = CargoKillOptions,
  path = "/processes/{kind}/{key}/kill",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key (jobs use their non-namespaced key)", example = "deploy-example.global"),
  ),
  responses(
    (status = 200, description = "Process instances killed"),
  ),
))]
#[web::post("/processes/{kind}/{key}/kill")]
pub async fn kill_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  payload: web::types::Json<CargoKillOptions>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  utils::container::process::kill_by_kind_key(&key, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}

/// Send a signal to a single process by its name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = CargoKillOptions,
  path = "/processes/{name}/kill",
  params(
    ("name" = String, Path, description = "Name or id of the container", example = "nstore.system.c"),
  ),
  responses(
    (status = 200, description = "Process killed"),
  ),
))]
#[web::post("/processes/{name}/kill")]
pub async fn kill_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoKillOptions>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  state
    .inner
    .docker_api
    .kill_container(&name, Some(payload.into_inner().into()))
    .await?;
  Ok(web::HttpResponse::Ok().finish())
}
