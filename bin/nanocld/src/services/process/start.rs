use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use crate::{models::SystemState, utils};

/// Start all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{key}/start",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
  ),
  responses(
    (status = 202, description = "Process instances started"),
  ),
))]
pub async fn start_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  utils::container::generic::emit_starting(&key, &kind, &state).await?;
  Ok(web::HttpResponse::Accepted().build())
}
