use ntex::web;
use tokio::fs::File;
use ntex::http::StatusCode;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::vm_image::VmImageImportContext;

use crate::error::HttpResponseError;

#[web::post("/vms/images/{name}/base")]
async fn create_base_image(
  mut payload: web::types::Payload,
  daemon_config: web::types::State<DaemonConfig>,
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let name = path.1.to_owned();
  let state_dir = daemon_config.state_dir.clone();
  let vm_images_dir = format!("{state_dir}/vms/images");
  let filepath = format!("{vm_images_dir}/{name}");
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
    let mut payload = match serde_json::to_vec(&VmImageImportContext {
      writed: bytes.len(),
    }) {
      Err(err) => {
        return Err(HttpResponseError {
          status: StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("Error while trying to white file at {filepath}: {err}"),
        });
      }
      Ok(payload) => payload,
    };
    payload.push(b'\n');
    println!("Writing : {}", bytes.len());
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

  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_base_image);
}
