use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmDb},
  objects::generic::*,
  utils,
};

/// Get detailed information about a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "Vms",
  path = "/vms/{key}/inspect",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{name}.{namespace}` format"),
  ),
  responses(
    (status = 200, description = "Detailed information about a virtual machine", body = nanocl_stubs::vm::VmInspect),
    (status = 404, description = "The virtual machine does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::get("/vms/{key}/inspect")]
pub async fn inspect_vm(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  let vm = VmDb::inspect_obj_by_pk(key.as_str(), &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm))
}
