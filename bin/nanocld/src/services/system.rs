use bollard_next::network::InspectNetworkOptions;
use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::system::{BinaryInfo, HostInfo};

use crate::models::SystemState;
use crate::vars;

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
  let docker = state.inner.docker_api.info().await?;
  let host_gateway = state.inner.config.gateway.clone();
  let network = state
    .inner
    .docker_api
    .inspect_network("nanoclbr0", None::<InspectNetworkOptions<String>>)
    .await?;
  let info = HostInfo {
    docker,
    host_gateway,
    network,
    config: state.inner.config.clone(),
  };
  Ok(web::HttpResponse::Ok().json(&info))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(get_ping);
  config.service(get_version);
  config.service(get_info);
}

#[cfg(test)]
mod tests {
  use nanocl_stubs::system::HostInfo;
  use ntex::http;

  use crate::services::ntex_config;
  use crate::utils::tests::*;

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
