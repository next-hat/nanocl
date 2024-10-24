use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{generic::GenericNspQuery, vm_spec::VmSpecUpdate};

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
  path = "/vms/{name}",
  params(
    ("name" = String, Path, description = "Name of the virtual machine"),
    ("namespace" = Option<String>, Query, description = "Namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "Updated virtual machine", body = Vm),
    (status = 404, description = "Virtual machine not found", body = ApiError),
  ),
))]
#[web::patch("/vms/{name}")]
pub async fn patch_vm(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<VmSpecUpdate>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &path.1);
  let version = path.0.clone();
  let obj = &VmObjPatchIn {
    spec: payload.into_inner(),
    version: version.clone(),
  };
  let vm = VmDb::patch_obj_by_pk(&key, obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm))
}
