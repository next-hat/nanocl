use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericNspQuery;

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
  path = "/cargoes/{name}/histories/{id}/revert",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("id" = String, Path, description = "Id of the cargo history"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs default to global namespace"),
  ),
  responses(
    (status = 200, description = "Cargo revert", body = Cargo),
    (status = 404, description = "Cargo does not exist", body = ApiError),
  ),
))]
#[web::patch("/cargoes/{name}/histories/{id}/revert")]
pub async fn revert_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, uuid::Uuid)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let cargo_key = utils::key::gen_key(&namespace, &path.1);
  let spec = SpecDb::read_by_pk(&path.2, &state.inner.pool)
    .await?
    .try_to_cargo_spec()?;
  let obj = &CargoObjPutIn {
    spec: spec.into(),
    version: path.0.clone(),
  };
  let cargo = CargoDb::put_obj_by_pk(&cargo_key, obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargo))
}
