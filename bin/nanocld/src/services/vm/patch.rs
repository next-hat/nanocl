use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::vm_spec::VmSpecUpdate;

use crate::{
  models::{SystemState, VmDb, VmObjPatchIn},
  objects::generic::*,
  utils,
};

/// Patch a virtual machine config meaning merging current config with the new one and add history entry
#[cfg_attr(feature = "dev", utoipa::path(
  patch,
  tag = "Vms",
  request_body = VmSpecUpdate,
  path = "/vms/{key}",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "Updated virtual machine", body = nanocl_stubs::vm::Vm),
    (status = 404, description = "Virtual machine not found", body = crate::services::openapi::ApiError),
  ),
))]
#[web::patch("/vms/{key}")]
pub async fn patch_vm(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<VmSpecUpdate>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  if payload
    .name
    .as_deref()
    .is_some_and(|name| name != key.name())
  {
    return Err(HttpError::bad_request(
      "VM names are immutable; create a new VM to use another name",
    ));
  }
  let version = path.0.clone();
  let obj = &VmObjPatchIn {
    spec: payload.into_inner(),
    version: version.clone(),
  };
  let vm = VmDb::patch_obj_by_pk(key.as_str(), obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm))
}
