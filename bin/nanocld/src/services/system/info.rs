use ntex::web;

use bollard_next::network::InspectNetworkOptions;
use nanocl_error::http::HttpResult;
use nanocl_stubs::system::HostInfo;

use crate::models::SystemState;

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
