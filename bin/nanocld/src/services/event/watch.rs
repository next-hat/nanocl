use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::system::EventCondition;

use crate::models::SystemState;

/// Watch on new events of all peer nodes with optional condition to stop the stream
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Events",
  path = "/events/watch",
  request_body = Option<Vec<EventCondition>>,
  responses(
    (status = 200, description = "Event stream", body = String, content_type = "text/event-stream"),
  ),
))]
pub async fn watch_event(
  state: &SystemState,
  _request_state: (),
  condition: Option<web::types::Json<Vec<EventCondition>>>,
) -> HttpResult<web::HttpResponse> {
  let stream = state
    .subscribe_raw(condition.map(|c| c.into_inner()))
    .await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("text/event-stream")
      .streaming(stream),
  )
}
