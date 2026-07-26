use ntex::web;

use bollard_next::exec::CreateExecOptions;
use nanocl_error::http::HttpResult;

use crate::{models::SystemState, utils};

// Create an exec command in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CreateExecOptions,
  path = "/cargoes/{cargo_key}/exec",
  params(
    ("cargo_key" = String, Path, description = "Canonical cargo key in `{name}.{namespace}` format"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::post("/cargoes/{cargo_key}/exec")]
pub async fn create_exec_command(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CreateExecOptions>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let result =
    utils::exec::create_exec_command(key.as_str(), &payload, &state).await?;
  Ok(web::HttpResponse::Ok().json(&result))
}
