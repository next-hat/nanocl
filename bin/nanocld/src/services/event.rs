use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  generic::{GenericCount, GenericListQuery},
  system::EventCondition,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{EventDb, SystemState},
};

/// Get events of all peer nodes
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

/// Watch on new events of all peer nodes with optional condition to stop the stream
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Events",
  path = "/events/watch",
  request_body = Option<Vec<EventCondition>>,
  responses(
    (status = 200, description = "Event stream", body = String),
  ),
))]
#[web::post("/events/watch")]
pub async fn watch_event(
  state: web::types::State<SystemState>,
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

/// Count events
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_event);
  config.service(watch_event);
  config.service(inspect_event);
  config.service(count_event);
}

#[cfg(test)]
mod tests {
  use ntex::{rt, http};
  use futures::{StreamExt, TryStreamExt};
  use bollard_next::container::Config;
  use nanocl_stubs::{
    cargo_spec::CargoSpecPartial,
    system::{
      Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
    },
  };

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut resp = client.get("/events").send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let events = resp.json::<Vec<Event>>().await.unwrap();
    assert!(!events.is_empty());
    let mut resp = client
      .get(&format!("/events/{}/inspect", events[0].key))
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    resp.json::<Event>().await.unwrap();
  }

  #[ntex::test]
  async fn watch_events() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_post("/events/watch", None::<String>, None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "watch events");
  }

  #[ntex::test]
  async fn watch_events_condition() {
    const CARGO_NAME: &str = "event-condition";
    let system = gen_default_test_system().await;
    let client = system.client;
    let client_ptr = client.clone();
    let conditions = [EventCondition {
      actor_kind: Some(EventActorKind::Cargo),
      actor_key: Some(format!("{CARGO_NAME}.global")),
      kind: [EventKind::Normal].to_vec(),
      action: [NativeEventAction::Start].to_vec(),
      ..Default::default()
    }];
    let wait_task = rt::spawn(async move {
      let res = client_ptr
        .send_post("/events/watch", Some(conditions), None::<String>)
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::OK,
        "watch events condition"
      );
      let mut stream = res.into_stream();
      while (stream.next().await).is_some() {}
    });
    let cargo = CargoSpecPartial {
      name: CARGO_NAME.to_owned(),
      container: Config {
        image: Some("alpine:latest".to_owned()),
        ..Default::default()
      },
      ..Default::default()
    };
    let _ = client
      .send_post("/cargoes", Some(cargo), None::<String>)
      .await;
    let _ = client
      .send_post(
        &format!("/processes/cargo/{CARGO_NAME}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert!(wait_task.await.is_ok())
  }
}
