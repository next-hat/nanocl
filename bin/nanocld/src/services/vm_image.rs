use ntex::web;
use ntex::http::StatusCode;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use nanocl_stubs::vm_image::VmImageResizePayload;

use crate::{repositories, utils};
use crate::error::HttpResponseError;
use crate::models::{Pool, VmImageDbModel, DaemonState};

#[web::post("/vms/images/{name}/import")]
async fn import_vm_image(
  mut payload: web::types::Payload,
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();

  utils::key::validate_name(&name)?;

  if repositories::vm_image::find_by_name(&name, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Vm image {name} already used"),
    });
  }

  let state_dir = state.config.state_dir.clone();
  let vm_images_dir = format!("{state_dir}/vms/images");
  let filepath = format!("{vm_images_dir}/{name}.img");
  let mut f = match fs::File::create(&filepath).await {
    Err(err) => {
      return Err(HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Error while trying to create file at {filepath}: {err}"),
      });
    }
    Ok(f) => f,
  };

  while let Some(bytes) = ntex::util::stream_recv(&mut payload).await {
    let bytes = match bytes {
      Err(err) => {
        log::error!("Unable to create vm image {name}: {err}");
        break;
      }
      Ok(bytes) => bytes,
    };
    if let Err(err) = f.write_all(&bytes).await {
      return Err(HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Error while trying to white file at {filepath}: {err}"),
      });
    }
  }

  if let Err(err) = f.shutdown().await {
    return Err(HttpResponseError {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error while closing file {filepath}: {err}"),
    });
  }

  // Get image info
  let image_info = match utils::vm_image::get_info(&filepath).await {
    Err(err) => {
      let _ = fs::remove_file(&filepath).await;
      return Err(err);
    }
    Ok(image_info) => image_info,
  };

  let vm_image = VmImageDbModel {
    name: name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Base".into(),
    format: image_info.format,
    size_actual: image_info.actual_size,
    size_virtual: image_info.virtual_size,
    path: filepath,
    parent: None,
  };

  repositories::vm_image::create(&vm_image, &state.pool).await?;

  Ok(web::HttpResponse::Ok().into())
}

#[web::get("/vms/images")]
async fn list_images(
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let images = repositories::vm_image::list(&state.pool).await?;

  Ok(web::HttpResponse::Ok().json(&images))
}

#[web::post("/vms/images/{name}/snapshot/{snapshot_name}")]
async fn create_vm_image_snapshot(
  path: web::types::Path<(String, String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let snapshot_name = path.2.to_owned();
  utils::key::validate_name(&snapshot_name)?;
  let image = repositories::vm_image::find_by_name(&name, &state.pool).await?;
  let vm_image = utils::vm_image::create_snap(
    &snapshot_name,
    50,
    &image,
    &state.config,
    &state.pool,
  )
  .await?;

  Ok(web::HttpResponse::Ok().json(&vm_image))
}

#[web::post("/vms/images/{name}/clone/{clone_name}")]
async fn clone_vm_image(
  path: web::types::Path<(String, String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let clone_name = path.2.to_owned();
  utils::key::validate_name(&clone_name)?;
  let image = repositories::vm_image::find_by_name(&name, &state.pool).await?;

  let rx =
    utils::vm_image::clone(&clone_name, &image, &state.config, &state.pool)
      .await?;

  Ok(web::HttpResponse::Ok().streaming(rx))
}

#[web::post("/vms/images/{name}/resize")]
async fn resize_vm_image(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
  web::types::Json(payload): web::types::Json<VmImageResizePayload>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();

  let rx = utils::vm_image::resize_by_name(&name, &payload, &pool).await?;

  Ok(web::HttpResponse::Ok().json(&rx))
}

#[web::delete("/vms/images/{name}")]
async fn delete_vm_image(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();

  utils::vm_image::delete(&name, &pool).await?;

  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(import_vm_image);
  config.service(list_images);
  config.service(delete_vm_image);
  config.service(create_vm_image_snapshot);
  config.service(clone_vm_image);
  config.service(resize_vm_image);
}
