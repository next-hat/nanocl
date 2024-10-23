use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{cargo_spec::CargoSpecUpdate, generic::GenericNspQuery};

use crate::{
  models::{CargoDb, CargoObjPatchIn, SystemState},
  objects::generic::*,
  utils,
};

/// Patch a cargo with it's specification meaning merging current spec with the new one and add history record
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  request_body = CargoSpecUpdate,
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs default to global namespace"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::patch("/cargoes/{name}")]
pub async fn patch_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecUpdate>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let obj = &CargoObjPatchIn {
    spec: payload.into_inner(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::patch_obj_by_pk(&key, obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
