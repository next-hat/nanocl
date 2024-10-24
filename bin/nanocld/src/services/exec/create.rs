use ntex::web;

use bollard_next::exec::CreateExecOptions;
use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericNspQuery;

use crate::{models::SystemState, utils};

// Create an exec command in a cargo
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  request_body = CreateExecOptions,
  path = "/cargoes/{cargo_name}/exec",
  params(
    ("Name" = String, Path, description = "Name of the cargo"),
    ("Namespace" = Option<String>, Query, description = "Namespace of the cargo"),
  ),
  responses(
    (status = 200, description = "Event Stream of the command output", content_type = "text/event-stream"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::post("/cargoes/{cargo_name}/exec")]
pub async fn create_exec_command(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CreateExecOptions>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let result = utils::exec::create_exec_command(&key, &payload, &state).await?;
  Ok(web::HttpResponse::Ok().json(&result))
}
