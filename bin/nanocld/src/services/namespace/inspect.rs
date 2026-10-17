use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{NamespaceDb, SystemState},
  objects::generic::*,
};

/// Get detailed information about a namespace
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Namespaces",
  path = "/namespaces/{name}/inspect",
  params(
    ("name" = String, Path, description = "The namespace name to inspect")
  ),
  responses(
    (status = 200, description = "Detailed information about a namespace", body = [nanocl_stubs::namespace::NamespaceInspect]),
    (status = 404, description = "Namespace doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_namespace(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let namespace = NamespaceDb::inspect_obj_by_pk(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&namespace))
}
