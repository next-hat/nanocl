use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericListQuery;

use crate::{
  models::{EventDb, SystemState},
  repositories::generic::*,
  utils,
};

/// List events with optional filter
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Events",
  path = "/events",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"filter\": { \"where\": { \"kind\": { \"eq\": \"normal\" } } } }"),
  ),
  responses(
    (status = 200, description = "List of events", body = Vec<Event>),
  ),
))]
#[web::get("/events")]
pub async fn list_event(
  state: web::types::State<SystemState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = utils::query_string::parse_qs_filter(&qs)?;
  let events = EventDb::transform_read_by(&filter, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&events))
}
