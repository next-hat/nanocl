use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::cargo::CargoDeleteQuery;

use crate::{
  models::{CargoDb, SystemState},
  objects::generic::*,
  utils,
};

/// Delete a cargo by its canonical key
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/cargoes/{key}",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{name}.{namespace}` format"),
    ("force" = bool, Query, description = "If true forces the delete operation even if the cargo is started"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::delete("/cargoes/{key}")]
pub async fn delete_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<CargoDeleteQuery>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  CargoDb::del_obj_by_pk(key.as_str(), &qs, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}
