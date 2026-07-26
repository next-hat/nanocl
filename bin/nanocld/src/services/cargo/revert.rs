use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use crate::{
  models::{CargoDb, CargoObjPutIn, SpecDb, SystemState},
  objects::generic::*,
  repositories::generic::*,
  utils,
};

/// Revert a cargo to a specific history record
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  path = "/cargoes/{cargo_key}/histories/{history_key}/revert",
  params(
    ("cargo_key" = String, Path, description = "Canonical cargo key in `{name}.{namespace}` format"),
    ("history_key" = String, Path, description = "Key of the cargo history"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = nanocl_stubs::cargo::Cargo),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::patch("/cargoes/{cargo_key}/histories/{history_key}/revert")]
pub async fn revert_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let cargo_key = utils::key::parse_resource_key(&path.1)?;
  let spec = SpecDb::read_by_pk(&path.2, &state.inner.pool)
    .await?
    .try_to_cargo_spec()?;
  if spec.name != cargo_key.name() {
    return Err(HttpError::bad_request(
      "Cannot revert a cargo to a history with another resource name",
    ));
  }
  let obj = &CargoObjPutIn {
    spec: spec.into(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(cargo_key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
