use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::cargo::CargoDeleteQuery;

use crate::{
  models::{CargoDb, SystemState},
  objects::generic::*,
  utils,
};

/// Delete a cargo by it's name
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Cargoes",
  path = "/cargoes/{name}",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs default to global namespace"),
    ("force" = bool, Query, description = "If true forces the delete operation even if the cargo is started"),
  ),
  responses(
    (status = 202, description = "Cargo deleted"),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::delete("/cargoes/{name}")]
pub async fn delete_cargo(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<CargoDeleteQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  CargoDb::del_obj_by_pk(&key, &qs, &state).await?;
  Ok(web::HttpResponse::Accepted().finish())
}
