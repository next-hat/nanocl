use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::vm::VmResizeOptions;
use ntex::web;

use crate::{models::SystemState, utils};

/// Grow running QEMU resources without restarting the VM.
///
/// Changes are not atomic. After an error, query resources before retrying;
/// QEMU capacity does not confirm guest CPU/memory online state or filesystem growth.
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms/{key}/resize",
  request_body = VmResizeOptions,
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 200, description = "Observed QEMU resources and CPU/memory persistence", body = nanocl_stubs::vm::VmResources),
    (status = 400, description = "Invalid resource totals, shrink request, or boot ceiling exceeded", body = crate::services::openapi::ApiError),
    (status = 403, description = "Trusted operator transport and matching Origin required", body = crate::services::openapi::ApiError),
    (status = 404, description = "VM does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "Runtime is unavailable, incompatible, or another VM operation is active", body = crate::services::openapi::ApiError),
    (status = 500, description = "Resize or persistence failed; changes may be partially applied, query resources before retrying", body = crate::services::openapi::ApiError),
    (status = 504, description = "Resize timed out; changes may be partially applied, query resources before retrying", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn resize_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<VmResizeOptions>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  super::console::authorize_request(&req)?;
  let key = utils::key::parse_resource_key(&path.1)?;
  let options = payload.into_inner();
  options.validate().map_err(HttpError::bad_request)?;
  let resources = utils::container::vm::resize(key.as_str(), &options, state)
    .await
    .map_err(super::lifecycle::lifecycle_error)?;
  Ok(web::HttpResponse::Ok().json(&resources))
}

/// Query QEMU resource totals, boot ceilings, and CPU/memory persistence.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/resources",
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 200, description = "Observed QEMU resources and CPU/memory persistence", body = nanocl_stubs::vm::VmResources),
    (status = 400, description = "Invalid canonical VM key", body = crate::services::openapi::ApiError),
    (status = 403, description = "Trusted operator transport and matching Origin required", body = crate::services::openapi::ApiError),
    (status = 404, description = "VM does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "Runtime is unavailable or incompatible", body = crate::services::openapi::ApiError),
    (status = 504, description = "QMP did not respond in time", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn vm_resources(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  req: web::HttpRequest,
) -> HttpResult<web::HttpResponse> {
  super::console::authorize_request(&req)?;
  let key = utils::key::parse_resource_key(&path.1)?;
  let resources = utils::container::vm::runtime_resources(key.as_str(), state)
    .await
    .map_err(super::lifecycle::lifecycle_error)?;
  Ok(web::HttpResponse::Ok().json(&resources))
}
