use ntex::web;

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::generic::{GenericListQuery, GenericFilter};

use crate::{
  repositories::generic::*,
  models::{EventDb, SystemState},
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
pub async fn list_event(
  state: web::types::State<SystemState>,
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

#[cfg(test)]
mod tests {
  use ntex::http::StatusCode;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    let resp = client.get("/events").send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
  }
}
