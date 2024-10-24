use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQuery};

use crate::{
  models::{ResourceDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count resources
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"name\": { \"eq\": \"global\" } } } }"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/resources/count")]
pub async fn count_resource(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let count = ResourceDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
