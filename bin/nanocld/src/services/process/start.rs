use ntex::web;

use bollard_next::container::StartContainerOptions;
use nanocl_error::http::{HttpError, HttpResult};

use crate::{
  models::{ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Start a single process by it's name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{pk}/start",
  params(
    ("pk" = String, Path, description = "Pk of the process", example = "1234567890"),
  ),
  responses(
    (status = 202, description = "Process instances started"),
  ),
))]
#[web::post("/processes/{pk}/start")]
pub async fn start_process_by_pk(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, pk) = path.into_inner();
  let process = ProcessDb::read_by_pk(&pk, &state.inner.pool).await?;
  state
    .inner
    .docker_api
    .start_container(&process.key, None::<StartContainerOptions<String>>)
    .await?;
  Ok(web::HttpResponse::Accepted().finish())
}

/// Start all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{key}/start",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key (jobs use their non-namespaced key)", example = "deploy-example.global"),
  ),
  responses(
    (status = 202, description = "Process instances started"),
  ),
))]
#[web::post("/processes/{kind}/{key}/start")]
pub async fn start_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  utils::container::generic::emit_starting(&key, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}
