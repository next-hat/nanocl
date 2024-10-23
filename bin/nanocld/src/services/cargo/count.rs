use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericClause, GenericCount, GenericListQueryNsp};

use crate::{
  models::{CargoDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count cargoes with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"CPU\" } } } }"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargoes are"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/cargoes/count")]
pub async fn count_cargo(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQueryNsp>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_nsp_filter(&qs)?;
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let filter = filter
    .filter
    .clone()
    .unwrap_or_default()
    .r#where("namespace_name", GenericClause::Eq(namespace));
  let count = CargoDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
