use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQueryNsp;

use crate::{
  models::{CargoDb, SystemState},
  utils,
};

/// List cargoes with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"test\" } } } }"),
    ("namespace" = Option<String>, Query, description = "Namespace where the cargoes are default to global namespace"),
  ),
  responses(
    (status = 200, description = "List of cargoes", body = [CargoSummary]),
  ),
))]
#[web::get("/cargoes")]
pub async fn list_cargo(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQueryNsp>,
) -> HttpResult<web::HttpResponse> {
  let query = utils::query_string::parse_qs_nsp_filter(&qs)?;
  let cargoes = CargoDb::list(&query, &state).await?;
  Ok(web::HttpResponse::Ok().json(&cargoes))
}
