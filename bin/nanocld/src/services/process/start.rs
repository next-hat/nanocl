use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use crate::{
  models::{JobStartQuery, SystemState},
  utils,
};
use nanocl_stubs::process::ProcessKind;

/// Start all processes for a kind and canonical resource key
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  path = "/processes/{kind}/{key}/start",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("key" = String, Path, description = "Canonical resource key in `{namespace}.{name}` format (jobs use their non-namespaced key)", example = "global.deploy-example"),
    ("incarnation" = Option<uuid::Uuid>, Query, description = "For Jobs, reject a stale scheduled invocation from another incarnation"),
  ),
  responses(
    (status = 202, description = "Process instances started"),
  ),
))]
pub async fn start_processes(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
  query: web::types::Query<JobStartQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, key) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  utils::key::validate_kind_key(&kind, &key).map_err(HttpError::bad_request)?;
  if kind == ProcessKind::Job {
    crate::tasks::job::start_if_incarnation(&key, query.incarnation, state)
      .await?;
  } else {
    utils::container::generic::emit_starting(&key, &kind, &state).await?;
  }
  Ok(web::HttpResponse::Accepted().build())
}
