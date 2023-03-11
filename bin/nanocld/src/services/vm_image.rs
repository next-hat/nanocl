use ntex::web;
use tokio::fs;
use tokio::fs::File;
use ntex::http::StatusCode;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use nanocl_stubs::config::DaemonConfig;

use crate::repositories;
use crate::error::HttpResponseError;
use crate::models::{Pool, VmImageDbModel};

#[web::post("/vms/images/{name}/base")]
async fn create_base_image(
  mut payload: web::types::Payload,
  daemon_config: web::types::State<DaemonConfig>,
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();

  // Ensure name only contain a-z, A-Z, 0-9, - and _
  if !name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
  {
    return Err(HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Vm image name {name} is invalid"),
    });
  }

  if repositories::vm_image::find_by_name(&name, &pool)
    .await
    .is_ok()
  {
    return Err(HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("Vm image {name} already used"),
    });
  }

  let state_dir = daemon_config.state_dir.clone();
  let vm_images_dir = format!("{state_dir}/vms/images");
  let filepath = format!("{vm_images_dir}/{name}.img");
  let mut f = match File::create(&filepath).await {
    Err(err) => {
      return Err(HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!("Error while trying to create file at {filepath}: {err}"),
      });
    }
    Ok(f) => f,
  };

  while let Some(bytes) = payload.next().await {
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

  // Get the file size
  let metadata = match tokio::fs::metadata(&filepath).await {
    Err(err) => {
      return Err(HttpResponseError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        msg: format!(
          "Error while trying to get file metadata at {filepath}: {err}"
        ),
      });
    }
    Ok(metadata) => metadata,
  };

  let vm_image = VmImageDbModel {
    name: name.clone(),
    created_at: chrono::Utc::now().naive_utc(),
    kind: "Base".into(),
    size: metadata.len() as i64,
    path: filepath,
    parent: None,
  };

  repositories::vm_image::create(vm_image, &pool).await?;

  Ok(web::HttpResponse::Ok().into())
}

#[web::get("/vms/images")]
async fn list_images(
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let images = repositories::vm_image::list(&pool).await?;

  Ok(web::HttpResponse::Ok().json(&images))
}

#[web::delete("/vms/images/{name}")]
async fn delete_vm_image(
  pool: web::types::State<Pool>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();

  let vm_image = repositories::vm_image::find_by_name(&name, &pool).await?;

  let filepath = vm_image.path.clone();

  if let Err(err) = fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }

  repositories::vm_image::delete_by_name(&name, &pool).await?;

  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_base_image);
  config.service(list_images);
  config.service(delete_vm_image);
}
