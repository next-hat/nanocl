use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmImageDb},
  repositories::generic::*,
};

/// Get detailed information about a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "VmImages",
  path = "/vms/images/{name}/inspect",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
  ),
  responses(
    (status = 200, description = "Detailed information about the vm image", body = VmImage),
  ),
))]
#[web::get("/vms/images/{name}/inspect")]
pub async fn inspect_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let item = VmImageDb::read_by_pk(&name, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&item))
}
