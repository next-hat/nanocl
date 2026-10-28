use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{CargoDb, SystemState},
  utils,
};

/// Revert a cargo to a specific history record
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Cargoes",
  path = "/cargoes/{cargo_key}/histories/{history_key}/revert",
  params(
    ("cargo_key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
    ("history_key" = String, Path, description = "Key of the cargo history"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = nanocl_stubs::cargo::Cargo),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn revert_cargo(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let cargo_key = utils::key::parse_resource_key(&path.1)?;
  let cargo =
    CargoDb::revert_to_history(cargo_key.as_str(), path.2, &path.0, state)
      .await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
