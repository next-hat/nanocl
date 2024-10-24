use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{
  models::{SystemState, VmImageDb},
  repositories::generic::*,
  utils,
};

/// Create a snapshot of a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "VmImages",
  request_body = String,
  path = "/vms/images/{name}/snapshot/{snapshot_name}",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
    ("snap" = String, Path, description = "The name of the snapshot"),
  ),
  responses(
    (status = 200, description = "The snapshot have been created", body = VmImage),
  ),
))]
#[web::post("/vms/images/{name}/snapshot/{snapshot_name}")]
pub async fn snapshot_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let snapshot_name = path.2.to_owned();
  utils::key::validate_name(&snapshot_name)?;
  let image = VmImageDb::read_by_pk(&name, &state.inner.pool).await?;
  let vm_image =
    utils::vm_image::create_snap(&snapshot_name, 50, &image, &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm_image))
}
