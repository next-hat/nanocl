use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::system::{EventCondition, HostInfo};

use crate::vars;
use crate::models::SystemState;

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  head,
  tag = "System",
  path = "/_ping",
  responses(
    (status = 202, description = "Server is up"),
  ),
))]
#[web::head("/_ping")]
pub async fn get_ping() -> HttpResult<web::HttpResponse> {
  Ok(web::HttpResponse::Accepted().into())
}

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/version",
  responses(
    (status = 200, description = "Version information", body = BinaryInfo),
  ),
))]
#[web::get("/version")]
pub async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&serde_json::json!({
    "Arch": vars::ARCH,
    "Channel": vars::CHANNEL,
    "Version": vars::VERSION,
    "CommitId": vars::COMMIT_ID,
  }))
}

/// Get host/node system information
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/info",
  responses(
    (status = 200, description = "Host/Node information", body = HostInfo),
  ),
))]
#[web::get("/info")]
pub async fn get_info(
  state: web::types::State<SystemState>,
) -> HttpResult<web::HttpResponse> {
  let docker = state.docker_api.info().await?;
  let host_gateway = state.config.gateway.clone();
  let info = HostInfo {
    docker,
    host_gateway,
    config: state.config.clone(),
  };
  Ok(web::HttpResponse::Ok().json(&info))
}

/// Watch on new events using Server-Sent Events / EventSource
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "System",
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
  config.service(get_ping);
  config.service(get_version);
  config.service(get_info);
  config.service(watch_event);
}

#[cfg(test)]
mod tests {

  use bollard_next::container::Config;
  use futures_util::{StreamExt, TryStreamExt};
  use nanocl_stubs::cargo_spec::CargoSpecPartial;
  use ntex::{http, rt};

  use nanocl_stubs::system::{
    EventActorKind, EventCondition, EventKind, HostInfo, NativeEventAction,
  };

  use crate::services::ntex_config;
  use crate::utils::tests::*;

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

  #[ntex::test]
  async fn system_info() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut res = client.send_get("/info", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "system info");
    let _ = res.json::<HostInfo>().await.unwrap();
  }

  #[ntex::test]
  async fn wrong_version() {
    let client = gen_test_system(ntex_config, "13.44").await.client;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 13.44"
    );
    let client = gen_test_system(ntex_config, "5.2").await.client;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 5.2"
    );
  }

  #[ntex::test]
  async fn ping() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }
}
