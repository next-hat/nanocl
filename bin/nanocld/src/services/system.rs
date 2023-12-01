use nanocl_stubs::generic::GenericFilter;
use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::system::{HostInfo, ProccessQuery};

use crate::version;
use crate::models::{DaemonState, ContainerDb, Repository, NodeDb};

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
) -> HttpResult<web::HttpResponse> {
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
    ("all" = bool, Query, description = "Return instances from all nodes"),
    ("last" = Option<isize>, Query, description = "Return this number of most recently created containers"),
    ("namespace" = Option<String>, Query, description = "Return instances from this namespace only"),
  ),
  responses(
    (status = 200, description = "List of instances", body = [NodeContainerSummary]),
  ),
))]
#[web::get("/processes")]
pub(crate) async fn get_processes(
  state: web::types::State<DaemonState>,
  _: web::types::Query<ProccessQuery>,
) -> HttpResult<web::HttpResponse> {
  let nodes = NodeDb::find(&GenericFilter::default(), &state.pool).await??;
  let nodes = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<String, _>>();
  let instances = ContainerDb::find(&GenericFilter::default(), &state.pool)
    .await??
    .into_iter()
    .map(|instance| NodeContainerSummary {
      node: instance.node_id.clone(),
      ip_address: match nodes.get(&instance.node_id) {
        Some(node) => node.ip_address.clone(),
        None => "Unknow".to_owned(),
      },
      container: instance.data,
    })
    .collect::<Vec<NodeContainerSummary>>();
  Ok(web::HttpResponse::Ok().json(&instances))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
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
