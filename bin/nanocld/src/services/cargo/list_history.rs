use ntex::web;

use nanocl_error::{http::HttpResult, io::IoResult};
use nanocl_stubs::generic::GenericNspQuery;

use crate::{
  models::{SpecDb, SystemState},
  utils,
};

/// List cargo histories
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/{name}/histories",
  params(
    ("name" = String, Path, description = "Name of the cargo"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargo belongs"),
  ),
  responses(
    (status = 200, description = "List of cargo histories", body = Vec<CargoSpec>),
    (status = 404, description = "Cargo does not exist"),
  ),
))]
#[web::get("/cargoes/{name}/histories")]
pub async fn list_cargo_history(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let histories = SpecDb::read_by_kind_key(&key, &state.inner.pool)
    .await?
    .into_iter()
    .map(|e| e.try_to_cargo_spec())
    .collect::<IoResult<Vec<_>>>()?;
  Ok(web::HttpResponse::Ok().json(&histories))
}
