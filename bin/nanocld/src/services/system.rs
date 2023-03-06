/*
* Endpoints for system information
*/
use std::sync::{Arc, Mutex};

use ntex::web;
use nanocl_stubs::{config::DaemonConfig, system::HostInfo};

use crate::event::EventEmitter;
use crate::error::HttpResponseError;

#[web::get("/info")]
async fn get_info(
  config: web::types::State<DaemonConfig>,
  docker_api: web::types::State<bollard_next::Docker>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let docker = docker_api.info().await?;
  let host_gateway = config.gateway.clone();
  let info = HostInfo {
    host_gateway,
    docker,
  };
  Ok(web::HttpResponse::Ok().json(&info))
}

/// Join events stream
#[web::get("/events")]
async fn watch_events(
  event_emitter: web::types::State<Arc<Mutex<EventEmitter>>>,
) -> Result<web::HttpResponse, HttpResponseError> {
  // TODO: spawn a future to lock the event_emitter and subscribe to the stream
  let stream = event_emitter.lock().unwrap().subscribe();

  Ok(
    web::HttpResponse::Ok()
      .content_type("text/event-stream")
      .streaming(stream),
  )
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(watch_events);
  config.service(get_info);
}

#[cfg(test)]
mod tests {
  use crate::services::ntex_config;

  use ntex::http::StatusCode;
  use nanocl_stubs::system::HostInfo;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn watch_events() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/v0.2/events").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    Ok(())
  }

  #[ntex::test]
  async fn test_system_info() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/v0.2/info").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let _ = resp
      .json::<HostInfo>()
      .await
      .expect("To receive a valid version json payload");
    Ok(())
  }

  #[ntex::test]
  async fn wrong_version() {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/v0.3/info").send().await.unwrap();
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::NOT_FOUND,
      "Expect status to be {} got {}",
      StatusCode::NOT_FOUND,
      status
    );
    let resp = srv.get("/v5.2/info").send().await.unwrap();
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::NOT_FOUND,
      "Expect status to be {} got {}",
      StatusCode::NOT_FOUND,
      status
    );
  }
}
