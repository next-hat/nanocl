use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{CargoDb, SystemState},
  objects::generic::*,
  utils,
};

/// Get detailed information about a cargo by its canonical key
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{key}/inspect",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "Cargo details", body = nanocl_stubs::cargo::CargoInspect),
  ),
))]
#[web::get("/cargoes/{key}/inspect")]
pub async fn inspect_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let cargo = CargoDb::inspect_obj_by_pk(key.as_str(), &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
