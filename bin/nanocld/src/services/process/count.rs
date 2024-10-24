use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQuery};

use crate::{
  models::{ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count processes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Processes",
  path = "/processes/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"global\" } } } }"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/processes/count")]
pub async fn count_process(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let count = ProcessDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
