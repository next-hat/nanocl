use std::collections::HashMap;

use ntex::web;

use bollard_next::container::ListContainersOptions;

use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::system::{HostInfo, ProccessQuery};

use nanocl_error::http::HttpError;

use crate::{version, repositories};
use crate::models::DaemonState;

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
pub(crate) async fn get_ping() -> Result<web::HttpResponse, HttpError> {
  Ok(web::HttpResponse::Accepted().into())
}

/// Get version information
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/version",
  responses(
    (status = 200, description = "Version information", body = Version),
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
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let docker = state.docker_api.info().await?;
  let host_gateway = state.config.gateway.clone();
  let info = HostInfo {
    docker,
    host_gateway,
    config: state.config.clone(),
  };
  Ok(web::HttpResponse::Ok().json(&info))
}

/// Listen on events using Server-Sent Events / EventSource
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/events",
  responses(
    (status = 200, description = "Event stream", body = String),
  ),
))]
#[web::get("/events")]
pub(crate) async fn watch_event(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let stream = state.event_emitter.subscribe().await?;
  Ok(
    web::HttpResponse::Ok()
      .content_type("text/event-stream")
      .streaming(stream),
  )
}

/// List instances (cargo/vm) including non running ones
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "System",
  path = "/processes",
  params(
    ("All" = bool, Query, description = "Return instances from all nodes"),
    ("Last" = Option<isize>, Query, description = "Return this number of most recently created containers"),
    ("Namespace" = Option<String>, Query, description = "Return instances from this namespace only"),
  ),
  responses(
    (status = 200, description = "List of instances", body = [NodeContainerSummary]),
  ),
))]
#[web::get("/processes")]
pub(crate) async fn get_processes(
  web::types::Query(qs): web::types::Query<ProccessQuery>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let label = "io.nanocl=enabled".into();
  let mut filters: HashMap<String, Vec<String>> = HashMap::new();
  let mut labels = vec![label];
  if let Some(namespace) = &qs.namespace {
    repositories::namespace::find_by_name(namespace, &state.pool).await?;
    labels.push(format!("io.nanocl.vnsp={}", namespace));
    labels.push(format!("io.nanocl.cnsp={}", namespace));
  }
  filters.insert("label".into(), labels);
  let opts = qs.clone().into();
  let options = Some(ListContainersOptions::<String> { filters, ..opts });
  let containers = state.docker_api.list_containers(options).await?;
  let mut process = containers
    .into_iter()
    .map(|c| {
      NodeContainerSummary::new(
        state.config.hostname.clone(),
        state.config.advertise_addr.clone(),
        c,
      )
    })
    .collect::<Vec<NodeContainerSummary>>();
  let nodes =
    repositories::node::list_unless(&state.config.hostname, &state.pool)
      .await?;
  if opts.all {
    for node in nodes {
      let client = node.to_http_client();
      let node_containers = match client
        .process(Some(ProccessQuery {
          all: false,
          namespace: qs.namespace.clone(),
          ..Default::default()
        }))
        .await
      {
        Ok(containers) => containers,
        Err(_) => continue,
      };
      process.extend(node_containers);
    }
  }
  Ok(web::HttpResponse::Ok().json(&process))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(watch_event);
  config.service(get_info);
  config.service(get_processes);
  config.service(get_ping);
  config.service(get_version);
}

#[cfg(test)]
mod tests {

  use ntex::http;

  use nanocl_stubs::node::NodeContainerSummary;
  use nanocl_stubs::system::{HostInfo, ProccessQuery};

  use crate::services::ntex_config;
  use crate::utils::tests::*;

  #[ntex::test]
  async fn watch_events() {
    let client = gen_default_test_client().await;
    let res = client.send_get("/events", None::<String>).await;
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
    let client = gen_test_client(ntex_config, "12.44").await;
    let res = client.send_get("/info", None::<String>).await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "wrong version 12.44"
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

  #[ntex::test]
  async fn process() {
    let client = gen_default_test_client().await;
    let mut res = client
      .send_get(
        "/processes",
        Some(&ProccessQuery {
          all: false,
          ..Default::default()
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "processes");
    let _ = res.json::<Vec<NodeContainerSummary>>().await.unwrap();
  }
}
