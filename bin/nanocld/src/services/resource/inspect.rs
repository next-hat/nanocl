use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{ResourceDb, SystemState},
  repositories::generic::*,
};

/// Get detailed information about a resource
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}/inspect",
  params(
    ("name" = String, Path, description = "The resource name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a resource", body = nanocl_stubs::resource::Resource),
    (status = 404, description = "Resource doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_resource(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let resource =
    ResourceDb::transform_read_by_pk(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&resource))
}
