use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::generic::GenericNspQuery;

use crate::{
  models::{SystemState, VmDb},
  objects::generic::*,
  utils,
};

/// Delete a virtual machine by name
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Vms",
  path = "/vms/{name}",
  params(
    ("name" = String, Path, description = "The name of the virtual machine"),
    ("namespace" = Option<String>, Query, description = "The namespace of the virtual machine default to global namespace"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been deleted"),
  ),
))]
#[web::delete("/vms/{name}")]
pub async fn delete_vm(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  qs: web::types::Query<GenericNspQuery>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let namespace = utils::key::resolve_nsp(&qs.namespace);
  let key = utils::key::gen_key(&namespace, &name);
  VmDb::del_obj_by_pk(&key, &(), &state).await?;
  Ok(web::HttpResponse::Ok().finish())
}
