use std::io::Write;

use ntex::web;
use futures::StreamExt;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  generic::{GenericFilter, GenericListQuery},
  vm_image::VmImageResizePayload,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{SystemState, VmImageDb},
};

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
pub async fn list_vm_images(
  state: web::types::State<SystemState>,
  query: web::types::Query<GenericListQuery>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::try_from(query.into_inner())
    .map_err(|err| HttpError::bad_request(err.to_string()))?;
  let images = VmImageDb::read_by(&filter, &state.pool).await?;
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
pub async fn import_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  mut payload: web::types::Payload,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  utils::key::validate_name(&name)?;
  if VmImageDb::read_by_pk(&name, &state.pool).await.is_ok() {
    return Err(HttpError::conflict(format!("Vm image {name} already used")));
  }
  let state_dir = state.config.state_dir.clone();
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
pub async fn snapshot_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let snapshot_name = path.2.to_owned();
  utils::key::validate_name(&snapshot_name)?;
  let image = VmImageDb::read_by_pk(&name, &state.pool).await?;
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
pub async fn clone_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let clone_name = path.2.to_owned();
  utils::key::validate_name(&clone_name)?;
  let image = VmImageDb::read_by_pk(&name, &state.pool).await?;
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
pub async fn resize_vm_image(
  state: web::types::State<SystemState>,
  path: web::types::Path<(String, String)>,
  web::types::Json(payload): web::types::Json<VmImageResizePayload>,
) -> HttpResult<web::HttpResponse> {
  let name = path.1.to_owned();
  let rx =
    utils::vm_image::resize_by_name(&name, &payload, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&rx))
}

/// Get detailed information about a virtual machine image
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "VmImages",
  path = "/vms/images/{name}",
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
  let item = VmImageDb::read_by_pk(&name, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&item))
}

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
  let name = path.1.to_owned();
  utils::vm_image::delete_by_name(&name, &state.pool).await?;
  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(import_vm_image);
  config.service(list_vm_images);
  config.service(delete_vm_image);
  config.service(snapshot_vm_image);
  config.service(clone_vm_image);
  config.service(resize_vm_image);
  config.service(inspect_vm_image);
}

#[cfg(test)]
pub mod tests {
  use tokio_util::codec;
  use ntex::http::StatusCode;
  use futures_util::StreamExt;

  use nanocl_error::io::{IoResult, FromIo, IoError};
  use nanocl_stubs::vm_image::VmImage;

  use crate::utils::tests::*;

  async fn import_image(name: &str, path: &str) -> IoResult<()> {
    let client = gen_default_test_client().await;
    let file = tokio::fs::File::open(path).await?;
    let err_msg = format!("Unable to import image {name}:{path}");
    let stream =
      codec::FramedRead::new(file, codec::BytesCodec::new()).map(move |r| {
        let r = r?;
        let bytes = ntex::util::Bytes::from_iter(r.freeze().to_vec());
        Ok::<ntex::util::Bytes, std::io::Error>(bytes)
      });
    let mut res = client
      .post(&format!("/vms/images/{name}/import"))
      .send_stream(stream)
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    let status = res.status();
    if status != StatusCode::OK {
      let error = res
        .json::<serde_json::Value>()
        .await
        .map_err(|err| err.map_err_context(|| &err_msg))?;
      println!("{:?}", error);
    }
    test_status_code!(res.status(), StatusCode::OK, &err_msg);
    Ok(())
  }

  async fn inspect_image(name: &str) -> IoResult<VmImage> {
    let client = gen_default_test_client().await;
    let err_msg = format!("Unable to inspect image {name}");
    let mut res = client
      .get(&format!("/vms/images/{name}/inspect"))
      .send()
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    if res.status() != StatusCode::OK {
      return Err(IoError::not_found("vm_image", name));
    }
    test_status_code!(res.status(), StatusCode::OK, &err_msg);
    let data = res
      .json::<VmImage>()
      .await
      .map_err(|err| err.map_err_context(|| &err_msg))?;
    Ok(data)
  }

  pub async fn ensure_test_image() {
    let name = "ubuntu-22-test";
    let path = "../../tests/ubuntu-22.04-minimal-cloudimg-amd64.img";
    if inspect_image(name).await.is_ok() {
      return;
    }
    import_image(name, path).await.unwrap();
  }

  #[ntex::test]
  async fn basic() {
    let client = gen_default_test_client().await;
    let name = "ubuntu-22-test-basic";
    let path = "../../tests/ubuntu-22.04-minimal-cloudimg-amd64.img";
    import_image(name, path).await.unwrap();
    let image = inspect_image(name).await.unwrap();
    assert_eq!(image.name, name);
    let mut res = client.get("/vms/images").send().await.unwrap();
    test_status_code!(res.status(), StatusCode::OK, "Unable to list images");
    let images = res.json::<Vec<VmImage>>().await.unwrap();
    assert!(images.iter().any(|i| i.name == name));
    let res = client
      .delete(&format!("/vms/images/{name}"))
      .send()
      .await
      .unwrap();
    test_status_code!(res.status(), StatusCode::OK, "Unable to delete image");
  }
}
