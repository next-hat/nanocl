use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{NetworkDb, SystemState},
  repositories::generic::RepositoryReadByTransform,
};

/// Inspect a persisted Docker network by its deterministic key.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Networks",
  path = "/networks/{key}/inspect",
  params(
    ("key" = String, Path, description = "Network key in the form node_name.network_name"),
  ),
  responses(
    (status = 200, description = "Network details", body = nanocl_stubs::network::Network),
    (status = 404, description = "Network not found", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/networks/{key}/inspect")]
pub async fn inspect_network(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let network =
    NetworkDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&network))
}
