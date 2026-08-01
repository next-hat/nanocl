use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::cargo_spec::CargoSpecPartial;

use crate::{
  models::{CargoDb, CargoObjPutIn, SystemState},
  objects::generic::*,
  utils,
};

/// Update a cargo by it's new specification and create a history record
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Cargoes",
  request_body = CargoSpecPartial,
  path = "/cargoes/{key}",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = nanocl_stubs::cargo::Cargo),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::put("/cargoes/{key}")]
pub async fn put_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecPartial>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  if payload.name != key.name() {
    return Err(HttpError::bad_request(
      "Cargo names are immutable; create a new cargo to use another name",
    ));
  }
  let obj = &CargoObjPutIn {
    spec: payload.into_inner(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
