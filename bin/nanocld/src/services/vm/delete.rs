use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmDb},
  objects::generic::*,
  utils,
};

/// Delete a virtual machine by canonical key
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Vms",
  path = "/vms/{key}",
  params(
    ("key" = String, Path, description = "Canonical VM key in `{namespace}.{name}` format"),
  ),
  responses(
    (status = 200, description = "The virtual machine has been deleted"),
    (status = 404, description = "The virtual machine does not exist", body = crate::services::openapi::ApiError),
  ),
))]
#[web::delete("/vms/{key}")]
pub async fn delete_vm(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = utils::key::parse_resource_key(&path.1)?;
  VmDb::del_obj_by_pk(key.as_str(), &(), &state).await?;
  Ok(web::HttpResponse::Ok().finish())
}
