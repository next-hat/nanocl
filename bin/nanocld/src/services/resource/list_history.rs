use ntex::web;

use crate::models::{ResourceDb, SystemState};
use nanocl_error::http::HttpResult;

/// List resource history
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Resources",
  path = "/resources/{name}/histories",
  params(
    ("name" = String, Path, description = "The resource name to list history")
  ),
  responses(
    (status = 200, description = "The resource history", body = [nanocl_stubs::resource::ResourceSpec]),
    (status = 404, description = "Resource is not existing", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn list_resource_history(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let items = ResourceDb::history(&path.1, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&items))
}
