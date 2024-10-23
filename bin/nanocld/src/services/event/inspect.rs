use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{EventDb, SystemState},
  repositories::generic::*,
};

/// Get detailed information about an event
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Events",
  path = "/events/{key}/inspect",
  params(
    ("key" = String, Path, description = "Key of the event"),
  ),
  responses(
    (status = 200, description = "Detailed information about the event", body = Event),
  ),
))]
#[web::get("/events/{key}/inspect")]
pub async fn inspect_event(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let event = EventDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&event))
}
