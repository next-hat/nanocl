use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::vm_image::VmImageResizePayload;

use crate::{models::SystemState, utils};

/// Resize a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "VmImages",
  request_body = VmImageResizePayload,
  path = "/vms/images/{name}/resize",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
  ),
  responses(
    (status = 200, description = "The snapshot have been created", body = VmImage),
  ),
))]
#[web::post("/vms/images/{name}/resize")]
pub async fn resize_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  web::types::Json(payload): web::types::Json<VmImageResizePayload>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let rx =
    utils::vm_image::resize_by_name(&name, &payload, &state.inner.pool).await?;
  Ok(web::HttpResponse::Ok().json(&rx))
}
