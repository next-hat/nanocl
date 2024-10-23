use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{cargo_spec::CargoSpecPartial, generic::GenericNspQuery};

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
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "Cargo updated", body = Cargo),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::put("/cargoes/{name}")]
pub async fn put_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<CargoSpecPartial>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let obj = &CargoObjPutIn {
    spec: payload.into_inner(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(&key, obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
