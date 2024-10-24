use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{cargo::CargoKillOptions, generic::GenericNspQuery};

use crate::{models::SystemState, utils};

/// Send a signal to all processes of given kind and name (cargo, job, vm)
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Processes",
  request_body = CargoKillOptions,
  path = "/processes/{kind}/{name}/kill",
  params(
    ("kind" = String, Path, description = "Kind of the process", example = "cargo"),
    ("name" = String, Path, description = "Name of the process", example = "deploy-example"),
    ("namespace" = Option<String>, Query, description = "Namespace where the process belongs is needed"),
  ),
  responses(
    (status = 200, description = "Process instances killed"),
  ),
))]
#[web::post("/processes/{kind}/{name}/kill")]
pub async fn kill_processes(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
  payload: web::types::Json<CargoKillOptions>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let (_, kind, name) = path.into_inner();
  let kind = kind.parse().map_err(HttpError::bad_request)?;
  let kind_pk = utils::key::gen_kind_key(&kind, &name, &qs.namespace);
  utils::container::process::kill_by_kind_key(&kind_pk, &payload, &state)
    .await?;
  Ok(web::HttpResponse::Ok().into())
}
