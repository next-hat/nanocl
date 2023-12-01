use std::io::Write;

use ntex::{web, http};
use futures::StreamExt;

use nanocl_stubs::generic::{GenericFilter, GenericListQuery};
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::vm_image::VmImageResizePayload;

use crate::utils;
use crate::models::{DaemonState, VmImageDb, Repository};

/// List virtual machine images
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "VmImages",
  path = "/vms/images",
  params(
    ("filter" = Option<String>, Query, description = "Generic filter", example = "{ \"where\": { \"kind\": { \"eq\": \"Env\" } } }"),
  ),
  responses(
    (status = 200, description = "List of vm images", body = [VmImage]),
  ),
))]
#[web::get("/vms/images")]
pub(crate) async fn list_vm_images(
  state: web::types::State<DaemonState>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(query.into_inner())
    .map_err(|err| HttpError::bad_request(err.to_string()))?;
  let images = VmImageDb::find(&filter, &state.pool).await??;
  Ok(web::HttpResponse::Ok().json(&images))
}

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
pub(crate) async fn import_vm_image(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  mut payload: web::types::Payload,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  utils::key::validate_name(&name)?;
  if VmImageDb::find_by_pk(&name, &state.pool).await?.is_ok() {
    return Err(HttpError {
      status: http::StatusCode::BAD_REQUEST,
      msg: format!("Vm image {name} already used"),
    });
  }
  let state_dir = state.config.state_dir.clone();
  let vm_images_dir = format!("{state_dir}/vms/images");
  let filepath = format!("{vm_images_dir}/{name}.img");
  let fp = filepath.clone();
  let mut f = web::block(move || std::fs::File::create(fp))
    .await
    .map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to create vm image {name}: {err}"),
    })?;
  while let Some(bytes) = payload.next().await {
    let bytes = bytes.map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Unable to create vm image {name}: {err}"),
    })?;
    f = web::block(move || f.write_all(&bytes).map(|_| f))
      .await
      .map_err(|err| HttpError {
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Unable to create vm image {name}: {err}"),
      })?;
  }
  utils::vm_image::create(&name, &filepath, &state.pool).await?;
  Ok(web::HttpResponse::Ok().into())
}

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
pub(crate) async fn snapshot_vm_image(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let snapshot_name = path.2.to_owned();
  utils::key::validate_name(&snapshot_name)?;
  let image = VmImageDb::find_by_pk(&name, &state.pool).await??;
  let vm_image =
    utils::vm_image::create_snap(&snapshot_name, 50, &image, &state).await?;
  Ok(web::HttpResponse::Ok().json(&vm_image))
}

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
pub(crate) async fn clone_vm_image(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let clone_name = path.2.to_owned();
  utils::key::validate_name(&clone_name)?;
  let image = VmImageDb::find_by_pk(&name, &state.pool).await??;
  let rx = utils::vm_image::clone(&clone_name, &image, &state).await?;
  Ok(web::HttpResponse::Ok().streaming(rx))
}

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
pub(crate) async fn resize_vm_image(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
  web::types::Json(payload): web::types::Json<VmImageResizePayload>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let rx =
    utils::vm_image::resize_by_name(&name, &payload, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&rx))
}

/// Delete a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "VmImages",
  path = "/vms/images/{Name}",
  params(
    ("Name" = String, Path, description = "The name of the vm image"),
  ),
  responses(
    (status = 200, description = "Image have been deleted"),
  ),
))]
#[web::delete("/vms/images/{name}")]
pub(crate) async fn delete_vm_image(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  utils::vm_image::delete_by_name(&name, &state.pool).await?;
  Ok(web::HttpResponse::Ok().into())
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(import_vm_image);
  config.service(list_vm_images);
  config.service(delete_vm_image);
  config.service(snapshot_vm_image);
  config.service(clone_vm_image);
  config.service(resize_vm_image);
}
