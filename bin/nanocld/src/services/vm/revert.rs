use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmDb},
  utils,
};

/// Revert a virtual machine to a specific history record.
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Vms",
  path = "/vms/{vm_key}/histories/{history_key}/revert",
  params(
    ("vm_key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
    ("history_key" = uuid::Uuid, Path, description = "Specification UUID of the VM history"),
  ),
  responses(
    (status = 200, description = "Virtual machine revert accepted", body = nanocl_stubs::vm::Vm, headers(
      ("X-Nanocl-Operation" = String, description = "Accepted operation UUID. Match terminal event Metadata.Operation to confirm completion.")
    )),
    (status = 400, description = "Invalid key or history from another resource", body = crate::services::openapi::ApiError),
    (status = 404, description = "Virtual machine or history does not exist in the current incarnation", body = crate::services::openapi::ApiError),
    (status = 409, description = "Another VM lifecycle operation is active", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn revert_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<(String, String, uuid::Uuid)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let (vm, operation) =
    VmDb::revert_to_history_with_receipt(key.as_str(), path.2, &path.0, state)
      .await?;
  Ok(
    web::HttpResponse::Ok()
      .set_header("X-Nanocl-Operation", operation.to_string())
      .json(&vm),
  )
}
