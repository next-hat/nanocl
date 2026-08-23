use ntex::web;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::cargo_spec::CargoSpec;

use crate::{
  models::{CargoDb, CargoObjPutIn, SystemState},
  objects::generic::*,
  utils,
};

/// Update a cargo by it's new specification and create a history record
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Cargoes",
  request_body = CargoSpec,
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
  payload: web::types::Json<CargoSpec>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let spec = payload.into_inner();
  spec.validate().map_err(|error| {
    IoError::invalid_input("CargoSpec".to_owned(), error.to_string())
  })?;
  if spec.name != key.name() {
    return Err(HttpError::bad_request(
      "Cargo names are immutable; create a new cargo to use another name",
    ));
  }
  let obj = &CargoObjPutIn {
    spec,
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
