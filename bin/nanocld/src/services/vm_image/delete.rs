use ntex::web;

use nanocl_error::http::HttpResult;

use crate::{models::SystemState, utils};

/// Delete a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "VmImages",
  path = "/vms/images/{name}",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
  ),
  responses(
    (status = 200, description = "Image have been deleted"),
  ),
))]
#[web::delete("/vms/images/{name}")]
pub async fn delete_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let pk = path.1.to_owned();
  utils::vm_image::delete_by_pk(&pk, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}
