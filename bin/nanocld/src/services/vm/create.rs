use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::{generic::GenericNspQuery, vm_spec::VmSpecPartial};

use crate::{
  models::{SystemState, VmDb, VmObjCreateIn},
  objects::generic::*,
  utils,
};

/// Create a virtual machine
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Vms",
  path = "/vms",
  request_body = VmSpecPartial,
  params(
    ("namespace" = Option<String>, Query, description = "The namespace of the virtual machine"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been created", body = nanocl_stubs::vm::Vm),
    (status = 409, description = "The virtual machine already exists", body = crate::services::openapi::ApiError),
  ),
))]
pub async fn create_vm(
  state: &SystemState,
  _request_state: (),
  path: web::types::Path<String>,
  payload: web::types::Json<VmSpecPartial>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let obj = VmObjCreateIn {
    namespace,
    spec: payload.into_inner(),
    version: path.into_inner(),
  };
  let vm = VmDb::create_obj(&obj, &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm))
}
