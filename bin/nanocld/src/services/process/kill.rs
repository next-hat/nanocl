use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::process::ProcessKillOptions;

use crate::models::SystemState;

/// Send a signal to a single process by its name or id
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = ProcessKillOptions,
  path = "/processes/{name}/kill",
  params(
    ("name" = String, Path, description = "Name or id of the container", example = "system.nstore.c"),
  ),
  responses(
    (status = 200, description = "Process killed"),
  ),
))]
#[web::post("/processes/{name}/kill")]
pub async fn kill_process(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ProcessKillOptions>,
) -> HttpResult<web::HttpResponse> {
  let (_, name) = path.into_inner();
  state
    .inner
    .docker_api
    .kill_container(&name, Some(payload.into_inner().into()))
    .await?;
  Ok(web::HttpResponse::Ok().finish())
}
