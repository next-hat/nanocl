use std::io::Write;

use futures::StreamExt;
use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};

use crate::{
  models::{SystemState, VmImageDb},
  repositories::generic::*,
  utils,
};

/// Import a virtual machine image from a file
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "VmImages",
  request_body = String,
  path = "/vms/images/{name}/import",
  params(
    ("name" = String, Path, description = "The name of the vm image"),
  ),
  responses(
    (status = 200, description = "Image have been imported"),
  ),
))]
#[web::post("/vms/images/{name}/import")]
pub async fn import_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  mut payload: web::types::Payload,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  utils::key::validate_name(&name)?;
  if VmImageDb::read_by_pk(&name, &state.inner.pool)
    .await
    .is_ok()
  {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let state_dir = state.inner.config.state_dir.clone();
  let vm_images_dir = format!("{state_dir}/vms/images");
  let filepath = format!("{vm_images_dir}/{name}.img");
  let fp = filepath.clone();
  let mut f = web::block(move || std::fs::File::create(fp))
    .await
    .map_err(|err| {
      HttpError::internal_server_error(format!(
        "Unable to create vm image {name}: {err}"
      ))
    })?;
  while let Some(bytes) = payload.next().await {
    let bytes = bytes.map_err(|err| {
      HttpError::internal_server_error(format!(
        "Unable to create vm image {name}: {err}"
      ))
    })?;
    f = web::block(move || f.write_all(&bytes).map(|_| f))
      .await
      .map_err(|err| {
        HttpError::internal_server_error(format!(
          "Unable to create vm image {name}: {err}"
        ))
      })?;
  }
  utils::vm_image::create(&name, &filepath, &state).await?;
  Ok(web::HttpResponse::Ok().into())
}
