use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQueryNsp};

use crate::{
  models::{CargoDb, SystemState},
  utils,
};

/// Count cargoes with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Cargoes",
  path = "/cargoes/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"my-cargo\" } } } }"),
    ("namespace" = Option<String>, Query, description = "Optional namespace filter; omitted or blank returns all namespaces"),
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
  let query = utils::query_string::parse_qs_nsp_filter(&qs)?;
  let count = CargoDb::count(&query, &state).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
