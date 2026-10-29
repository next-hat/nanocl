use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::cargo::CargoDeleteQuery;

use crate::{
  models::{CargoDb, SystemState},
  utils,
};

/// Delete a cargo by its canonical key
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/cargoes/{key}",
  params(
    ("key" = String, Path, description = "Canonical cargo key in `{namespace}.{name}` format"),
    ("force" = bool, Query, description = "If true forces the delete operation even if the cargo is started"),
  ),
  responses(
    (status = 202, description = "Cargo deletion accepted", headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID. Match terminal event Metadata.Operation to confirm completion.")
    )),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn delete_cargo(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<CargoDeleteQuery>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let (_, operation) =
    CargoDb::delete_with_operation(key.as_str(), &qs, &state).await?;
  Ok(
    web::HttpResponse::Accepted()
      .set_header("X-Nanocl-Operation", operation.to_string())
      .build(),
  )
}
