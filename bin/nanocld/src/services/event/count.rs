use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::{GenericCount, GenericListQuery};

use crate::{
  models::{EventDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Count events with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Events",
  path = "/events/count",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"CPU\" } } } }"),
  ),
  responses(
    (status = 200, description = "Count result", body = GenericCount),
  ),
))]
#[web::get("/events/count")]
pub async fn count_event(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let count = EventDb::count_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&GenericCount { count }))
}
