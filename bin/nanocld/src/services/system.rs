use std::collections::HashMap;

use ntex::web;

use bollard_next::container::ListContainersOptions;
use nanocl_stubs::system::ProccessQuery;
use nanocl_stubs::system::HostInfo;

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::DaemonState;

#[web::get("/info")]
async fn get_info(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let docker = state.docker_api.info().await?;
  let host_gateway = state.config.gateway.clone();
  let info = HostInfo {
    host_gateway,
    docker,
  };
  Ok(web::HttpResponse::Ok().json(&info))
}

/// Join events stream
#[web::get("/events")]
async fn watch_events(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  // TODO: spawn a future to lock the event_emitter and subscribe to the stream
  let stream = state.event_emitter.lock().unwrap().subscribe();

  Ok(
    web::HttpResponse::Ok()
      .content_type("text/event-stream")
      .streaming(stream),
  )
}

#[web::get("/processes")]
async fn get_processes(
  web::types::Query(qs): web::types::Query<ProccessQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let label = "io.nanocl=enabled".into();
  let mut filters: HashMap<String, Vec<String>> = HashMap::new();
  let mut labels = vec![label];

  if let Some(namespace) = &qs.namespace {
    repositories::namespace::find_by_name(namespace.to_owned(), &state.pool)
      .await?;
    labels.push(format!("io.nanocl.vnsp={}", namespace));
    labels.push(format!("io.nanocl.cnsp={}", namespace));
  }

  filters.insert("label".into(), labels);

  let opts = qs.clone().into();

  let options = Some(ListContainersOptions::<String> { filters, ..opts });
  let containers = state.docker_api.list_containers(options).await?;

  Ok(web::HttpResponse::Ok().json(&containers))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(watch_events);
  config.service(get_info);
  config.service(get_processes);
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
  async fn system_info() -> TestRet {
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
    let resp = srv.get("/v12.44/info").send().await.unwrap();
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
