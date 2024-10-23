use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{cargo_spec::CargoSpecPartial, generic::GenericNspQuery};

use crate::{
  models::{CargoDb, CargoObjCreateIn, SystemState},
  objects::generic::*,
  utils,
};

/// Create a new cargo by it specification
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Cargoes",
  path = "/cargoes",
  request_body = CargoSpecPartial,
  params(
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 201, description = "Cargo created", body = Cargo),
  ),
))]
#[web::post("/cargoes")]
pub async fn create_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<String>,
  payload: web::types::Json<CargoSpecPartial>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let obj = CargoObjCreateIn {
    namespace: namespace.clone(),
    spec: payload.into_inner(),
    version: path.into_inner(),
  };
  let cargo = CargoDb::create_obj(&obj, &state).await?;
  Ok(web::HttpResponse::Created().json(&cargo))
}
