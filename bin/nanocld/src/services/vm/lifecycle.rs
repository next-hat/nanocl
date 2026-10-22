use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::vm::VmLifecycleOptions;
use ntex::{http::StatusCode, web};

use crate::{models::SystemState, utils};

pub(crate) fn lifecycle_error(error: IoError) -> HttpError {
  match error.inner.kind() {
    std::io::ErrorKind::TimedOut => {
      HttpError::new(StatusCode::GATEWAY_TIMEOUT, error)
    }
    std::io::ErrorKind::WouldBlock => HttpError::conflict(error),
    _ => error.into(),
  }
}

/// Shut down a VM and wait for confirmed runtime exit.
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms/{key}/stop",
  request_body = VmLifecycleOptions,
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 200, description = "VM runtime has stopped"),
    (status = 400, description = "Invalid canonical key or shutdown timeout", body = crate::services::openapi::ApiError),
    (status = 404, description = "VM does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "Runtime is stale or a lifecycle operation is already active", body = crate::services::openapi::ApiError),
    (status = 504, description = "VM operation timed out; check runtime status before retrying", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn stop_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<VmLifecycleOptions>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let options = payload.into_inner();
  options.validate().map_err(HttpError::bad_request)?;
  utils::container::vm::stop_with_options(key.as_str(), &options, state)
    .await
    .map_err(lifecycle_error)?;
  Ok(web::HttpResponse::Ok().build())
}

/// Shut down a VM, then start the same runtime after confirmed exit.
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms/{key}/restart",
  request_body = VmLifecycleOptions,
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 200, description = "VM runtime has restarted"),
    (status = 400, description = "Invalid canonical key or shutdown timeout", body = crate::services::openapi::ApiError),
    (status = 404, description = "VM does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "Runtime is stale or a lifecycle operation is already active", body = crate::services::openapi::ApiError),
    (status = 504, description = "VM operation timed out; check runtime status before retrying", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn restart_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<VmLifecycleOptions>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let options = payload.into_inner();
  options.validate().map_err(HttpError::bad_request)?;
  utils::container::vm::restart_with_options(key.as_str(), &options, state)
    .await
    .map_err(lifecycle_error)?;
  Ok(web::HttpResponse::Ok().build())
}

/// Query QEMU execution state independently of container running state.
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/status",
  params(("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format")),
  responses(
    (status = 200, description = "Actual QEMU execution state", body = nanocl_stubs::vm::VmRuntimeStatus),
    (status = 400, description = "Invalid canonical VM key", body = crate::services::openapi::ApiError),
    (status = 404, description = "VM does not exist", body = crate::services::openapi::ApiError),
    (status = 409, description = "VM runtime identity is stale or unavailable", body = crate::services::openapi::ApiError),
    (status = 504, description = "QMP did not respond in time", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn vm_status(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let status = utils::container::vm::runtime_status(key.as_str(), state)
    .await
    .map_err(lifecycle_error)?;
  Ok(web::HttpResponse::Ok().json(&status))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn vm_lifecycle_http_errors_preserve_timeout_and_conflict() {
    let error = IoError::with_context(
      "VM shutdown",
      std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "Guest did not shut down",
      ),
    );
    assert_eq!(lifecycle_error(error).status, StatusCode::GATEWAY_TIMEOUT);
    let error = IoError::with_context(
      "VM lifecycle",
      std::io::Error::new(std::io::ErrorKind::WouldBlock, "VM is busy"),
    );
    assert_eq!(lifecycle_error(error).status, StatusCode::CONFLICT);
    assert_eq!(
      lifecycle_error(IoError::invalid_input("VM timeout", "invalid")).status,
      StatusCode::BAD_REQUEST
    );
  }
}
