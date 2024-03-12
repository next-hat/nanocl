use ntex::web;

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::{
  generic::{GenericFilter, GenericListQuery},
  system::EventCondition,
};

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

/// Watch on new events using Server-Sent Events / EventSource
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

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_event);
  config.service(watch_event);
}

#[cfg(test)]
mod tests {
  use bollard_next::container::Config;
  use futures::{StreamExt, TryStreamExt};
  use ntex::{rt, http};
  use nanocl_stubs::{
    cargo_spec::CargoSpecPartial,
    system::{EventActorKind, EventCondition, EventKind, NativeEventAction},
  };

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let resp = client.get("/events").send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
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
