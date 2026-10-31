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
    (status = 200, description = "Cargo or VM is already started, verified under lifecycle admission"),
    (status = 202, description = "Start accepted", headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID for Cargo and VM; omitted for other kinds. Match terminal event Metadata.Operation to confirm completion.")
    )),
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
  let operation = match kind {
    ProcessKind::Job => {
      crate::tasks::job::start_if_incarnation(&key, query.incarnation, state)
        .await?;
      return Ok(web::HttpResponse::Accepted().build());
    }
    ProcessKind::Cargo => {
      crate::tasks::cargo::start_operation(&key, state).await?
    }
    ProcessKind::Vm => crate::tasks::vm::start_operation(&key, state).await?,
  };
  let Some(operation) = operation else {
    return Ok(web::HttpResponse::Ok().build());
  };
  let mut response = web::HttpResponse::Accepted();
  response.set_header("X-Nanocl-Operation", operation.to_string());
  Ok(response.build())
}
