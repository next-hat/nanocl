use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::system::HostInfo;

use crate::version;
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
pub(crate) async fn get_ping() -> HttpResult<web::HttpResponse> {
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
pub(crate) async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&serde_json::json!({
    "Arch": version::ARCH,
    "Channel": version::CHANNEL,
    "Version": version::VERSION,
    "CommitId": version::COMMIT_ID,
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
pub(crate) async fn get_info(
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
  get,
  tag = "System",
  path = "/events/watch",
  responses(
    (status = 200, description = "Event stream", body = String),
  ),
))]
#[web::get("/events/watch")]
pub(crate) async fn watch_event(
  state: web::types::State<SystemState>,
) -> HttpResult<web::HttpResponse> {
  let stream = state.event_manager.raw.subscribe().await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("text/event-stream")
      .streaming(stream),
  )
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_ping);
  config.service(get_version);
  config.service(get_info);
  config.service(watch_event);
}

#[cfg(test)]
mod tests {

  use ntex::http;

  use nanocl_stubs::system::HostInfo;

  use crate::services::ntex_config;
  use crate::utils::tests::*;

  #[ntex::test]
  async fn watch_events() {
    let client = gen_default_test_client().await;
    let res = client.send_get("/events/watch", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "watch events");
  }

  #[ntex::test]
  async fn system_info() {
    let client = gen_default_test_client().await;
    let mut res = client.send_get("/info", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "system info");
    let _ = res.json::<HostInfo>().await.unwrap();
  }

  #[ntex::test]
  async fn wrong_version() {
    let client = gen_test_client(ntex_config, "13.44").await;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 13.44"
    );
    let client = gen_test_client(ntex_config, "5.2").await;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 5.2"
    );
  }

  #[ntex::test]
  async fn ping() {
    let client = gen_default_test_client().await;
    let res = client.send_head("/_ping", None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::ACCEPTED, "ping");
  }
}
