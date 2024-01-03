use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::generic::{GenericListQuery, GenericFilter};
use ntex::web;

use crate::{
  repositories::generic::*,
  models::{EventDb, DaemonState},
};

/// Get events of all peer nodes
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Events",
  path = "/events",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"kind\": { \"eq\": \"normal\" } } }"),
  ),
  responses(
    (status = 200, description = "List of events", body = Vec<Event>),
  ),
))]
#[web::get("/events")]
pub(crate) async fn list_event(
  state: web::types::State<DaemonState>,
  qs: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(qs.into_inner()).map_err(|err| {
    HttpError::bad_request(format!("Invalid query string: {err}"))
  })?;
  let events = EventDb::transform_read_by(&filter, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&events))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_event);
}
