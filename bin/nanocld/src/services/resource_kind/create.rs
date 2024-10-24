use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::resource_kind::ResourceKindPartial;

use crate::models::{ResourceKindDb, SystemState};

/// Create a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "ResourceKinds",
  path = "/resource/kinds",
  request_body = ResourceKindPartial,
  responses(
    (status = 201, description = "Job created", body = ResourceKind),
  ),
))]
#[web::post("/resource/kinds")]
pub async fn create_resource_kind(
  state: web::types::State<SystemState>,
  _version: web::types::Path<String>,
  payload: web::types::Json<ResourceKindPartial>,
) -> HttpResult<web::HttpResponse> {
  let item =
    ResourceKindDb::create_from_spec(&payload, &state.inner.pool).await?;
  Ok(web::HttpResponse::Created().json(&item))
}
