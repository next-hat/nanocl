use ntex::web;

use bollard_next::exec::StartExecOptions;
use nanocl_error::http::HttpResult;

use crate::{models::SystemState, utils};

// Run an exec command
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Exec",
  request_body = StartExecOptions,
  path = "/exec/{id}/cargo/start",
  params(
    ("id" = String, Path, description = "Exec command id"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/exec/{id}/cargo/start")]
pub async fn start_exec_command(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<StartExecOptions>,
) -> HttpResult<web::HttpResponse> {
  utils::exec::start_exec_command(&path.1, &payload, &state).await
}
