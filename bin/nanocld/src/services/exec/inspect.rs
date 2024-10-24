use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{models::SystemState, utils};

/// Inspect a command executed in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Exec",
  path = "/exec/{id}/cargo/inspect",
  params(
    ("id" = String, Path, description = "Exec id to inspect"),
  ),
  responses(
    (status = 200, description = "Inspect exec infos", body = ExecInspectResponse),
    (status = 404, description = "Exec instance does not exist"),
  ),
))]
#[web::get("/exec/{id}/cargo/inspect")]
pub async fn inspect_exec_command(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let infos = utils::exec::inspect_exec_command(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&infos))
}
