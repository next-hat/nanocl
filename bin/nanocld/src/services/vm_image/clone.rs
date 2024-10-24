use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmImageDb},
  repositories::generic::*,
  utils,
};

/// Clone a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "VmImages",
  request_body = String,
  path = "/vms/images/{name}/clone/{clone_name}",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
    ("clone_name" = String, Path, description = "The name of the clone"),
  ),
  responses(
    (status = 200, description = "The snapshot have been created", body = VmImage),
  ),
))]
#[web::post("/vms/images/{name}/clone/{clone_name}")]
pub async fn clone_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let clone_name = path.2.to_owned();
  utils::key::validate_name(&clone_name)?;
  let image = VmImageDb::read_by_pk(&name, &state.inner.pool).await?;
  let rx = utils::vm_image::clone(&clone_name, &image, &state).await?;
  Ok(web::HttpResponse::Ok().streaming(rx))
}
