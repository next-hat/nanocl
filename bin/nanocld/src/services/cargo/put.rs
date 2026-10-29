use ntex::web;

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::cargo_spec::CargoSpec;

use crate::{
  models::{CargoDb, CargoObjPutIn, SystemState},
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
    (status = 200, description = "Cargo update accepted", body = nanocl_stubs::cargo::Cargo, headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID. Match terminal event Metadata.Operation to confirm completion.")
    )),
    (status = 404, description = "Cargo does not exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn put_cargo(
  state: &SystemState,
  _request_state: (),
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
  let (cargo, operation) =
    CargoDb::put_with_operation(key.as_str(), obj, &state).await?;
  Ok(
    web::HttpResponse::Ok()
      .set_header("X-Nanocl-Operation", operation.to_string())
      .json(&cargo),
  )
}
