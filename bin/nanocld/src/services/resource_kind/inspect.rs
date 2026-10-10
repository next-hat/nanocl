use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::resource_kind::ResourceKindVersion;

use crate::models::{ResourceKindDb, SpecDb, SystemState};

/// Get detailed information about a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}/inspect",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
  ),
  responses(
    (status = 200, description = "Details about a resource kind", body = nanocl_stubs::resource_kind::ResourceKindInspect),
    (status = 404, description = "Resource kind doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_resource_kind(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key: String = format!("{}/{}", path.1, path.2);
  let kind = ResourceKindDb::inspect_by_pk(&key, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&kind))
}

/// Get detailed information about a resource kind version
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}/version/{version}",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
    ("version" = String, Path, description = "Version of the resource kind"),
  ),
  responses(
    (status = 200, description = "Details about a resource kind", body = ResourceKindVersion),
    (status = 404, description = "Resource kind doesn't exist", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn inspect_resource_kind_version(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = format!("{}/{}", path.1, path.2);
  let kind_version =
    SpecDb::get_version(&key, &path.3, &state.inner.pool).await?;
  let kind_version: ResourceKindVersion = kind_version.try_into()?;
  Ok(web::HttpResponse::Ok().json(&kind_version))
}
