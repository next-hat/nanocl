use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::cargo_spec::CargoSpecUpdate;

use crate::{
  models::{CargoDb, CargoObjPatchIn, SystemState},
  objects::generic::*,
  utils,
};

/// Patch a cargo by canonical key and add a history record
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  request_body = CargoSpecUpdate,
  path = "/cargoes/{key}",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{name}.{namespace}` format"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = nanocl_stubs::cargo::Cargo),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::patch("/cargoes/{key}")]
pub async fn patch_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecUpdate>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  if payload
    .name
    .as_deref()
    .is_some_and(|name| name != key.name())
  {
    return Err(HttpError::bad_request(
      "Cargo names are immutable; create a new cargo to use another name",
    ));
  }
  let obj = &CargoObjPatchIn {
    spec: payload.into_inner(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::patch_obj_by_pk(key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
