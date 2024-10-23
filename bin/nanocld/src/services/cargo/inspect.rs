use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericNspQuery;

use crate::{
  models::{CargoDb, SystemState},
  objects::generic::*,
  utils,
};

/// Get detailed information about a cargo by its name
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{name}/inspect",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = CargoInspect),
  ),
))]
#[web::get("/cargoes/{name}/inspect")]
pub async fn inspect_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let cargo = CargoDb::inspect_obj_by_pk(&key, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
