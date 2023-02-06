/*
* Endpoints for system information
*/
use std::sync::{Arc, Mutex};

use nanocl_stubs::{config::DaemonConfig, system::HostInfo};
use ntex::web;
use serde_json::json;

use crate::{version, error::HttpResponseError, event::EventEmitter};

#[web::get("/version")]
async fn get_version() -> web::HttpResponse {
  web::HttpResponse::Ok().json(&json!({
    "Arch": version::ARCH,
    "Version": version::VERSION,
    "CommitId": version::COMMIT_ID,
  }))
}

#[web::get("/info")]
async fn get_info(
  config: web::types::State<DaemonConfig>,
  docker_api: web::types::State<bollard::Docker>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let docker = docker_api.info().await?;
  let host_gateway = config.host_gateway.clone();
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

// This is a dummy endpoint you can use to test if the server is accessible.
async fn ping() -> Result<web::HttpResponse, HttpResponseError> {
  Ok(web::HttpResponse::Ok().json(&json!({
    "msg": "pong",
  })))
}

pub async fn unhandled() -> Result<web::HttpResponse, HttpResponseError> {
  Err(HttpResponseError {
    status: ntex::http::StatusCode::NOT_FOUND,
    msg: "Route or method unhandled".into(),
  })
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(watch_events);
  config.service(get_version);
  config.service(get_info);
  config.service(
    web::resource("/_ping")
      .route(web::get().to(ping))
      .route(web::head().to(ping)),
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  use ntex::http::StatusCode;

  use nanocl_stubs::system::Version;

  use crate::utils::tests::*;

  #[ntex::test]
  pub async fn get_version() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/version").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let body: Version = resp
      .json()
      .await
      .expect("To receive a valid version json payload");
    assert_eq!(
      body.arch,
      version::ARCH,
      "Expect arch to be {}",
      version::ARCH
    );
    assert_eq!(
      body.version,
      version::VERSION,
      "Expect version to be {}",
      version::VERSION
    );
    assert_eq!(
      body.commit_id,
      version::COMMIT_ID,
      "Expect commit_id to be {}",
      version::COMMIT_ID
    );
    Ok(())
  }

  #[ntex::test]
  async fn watch_events() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/events").send().await?;
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
  async fn test_ping() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/_ping").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expect status to be {} got {}",
      StatusCode::OK,
      status
    );
    let body: serde_json::Value = resp
      .json()
      .await
      .expect("To receive a valid version json payload");
    assert_eq!(body["msg"], "pong");

    let resp = srv.head("/_ping").send().await?;
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
  async fn test_unhandled_route() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let resp = srv.get("/unhandled").send().await?;
    let status = resp.status();
    assert_eq!(
      status,
      StatusCode::NOT_FOUND,
      "Expect status to be {} got {}",
      StatusCode::NOT_FOUND,
      status
    );
    Ok(())
  }

  #[ntex::test]
  async fn test_system_info() -> TestRet {
    let srv = generate_server(ntex_config).await;
    let mut resp = srv.get("/info").send().await?;
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
}
