/*
* Endpoints to manipulate cargo images
*/
use ntex::web;
use ntex::http;
use futures::StreamExt;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio_util::codec;

use bollard_next::image::ImportImageOptions;

use nanocl_error::http::HttpError;
use nanocl_stubs::cargo_image::{
  CargoImagePartial, ListCargoImagesOptions, CargoImageImportOptions,
};

use crate::utils;
use crate::models::DaemonState;

/// List container images
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "CargoImages",
  path = "/cargoes/images",
  responses(
    (status = 200, description = "List of container image", body = [ImageSummary]),
  ),
))]
#[web::get("/cargoes/images")]
pub(crate) async fn list_cargo_image(
  web::types::Query(query): web::types::Query<ListCargoImagesOptions>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let images = utils::cargo_image::list(&query.into(), &state).await?;
  Ok(web::HttpResponse::Ok().json(&images))
}

/// Get detailed information about a container image
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/cargoes/images/{IdOrName}",
  tag = "CargoImages",
  params(
    ("IdOrName" = String, Path, description = "Image ID or name")
  ),
  responses(
    (status = 200, description = "Detailed information about an image", body = ImageInspect),
    (status = 404, description = "Image not found", body = ApiError),
  ),
))]
#[web::get("/cargoes/images/{id_or_name}*")]
pub(crate) async fn inspect_cargo_image(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let image = utils::cargo_image::inspect_by_name(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&image))
}

/// Download a container image
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = CargoImagePartial,
  tag = "CargoImages",
  path = "/cargoes/images",
  responses(
    (status = 200, description = "Download stream"),
    (status = 404, description = "Image not found", body = ApiError),
  ),
))]
#[web::post("/cargoes/images")]
pub(crate) async fn create_cargo_image(
  web::types::Json(payload): web::types::Json<CargoImagePartial>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let (from_image, tag) = utils::cargo_image::parse_image_info(&payload.name)?;
  let mut rx_body = utils::cargo_image::pull(&from_image, &tag, &state).await?;
  Ok(
    web::HttpResponse::Ok()
      .keep_alive()
      .content_type("application/vdn.nanocl.raw-stream")
      .streaming(rx_body),
  )
}

/// Delete a container image
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  path = "/cargoes/images/{IdOrName}",
  tag = "CargoImages",
  params(
    ("IdOrName" = String, Path, description = "Image ID or name")
  ),
  responses(
    (status = 200, description = "Delete response", body = GenericDelete),
    (status = 404, description = "Image not found", body = ApiError),
  ),
))]
#[web::delete("/cargoes/images/{id_or_name}*")]
pub(crate) async fn delete_cargo_image(
  path: web::types::Path<(String, String)>,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  let res = utils::cargo_image::delete(&path.1, &state).await?;
  Ok(web::HttpResponse::Ok().json(&res))
}

/// Import a container image from a tarball
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  request_body = String,
  tag = "CargoImages",
  path = "/cargoes/images/import",
  responses(
    (status = 200, description = "Image imported"),
    (status = 404, description = "Image not found", body = ApiError),
  ),
))]
#[web::post("/cargoes/images/import")]
pub(crate) async fn import_cargo_image(
  web::types::Query(query): web::types::Query<CargoImageImportOptions>,
  mut payload: web::types::Payload,
  state: web::types::State<DaemonState>,
) -> Result<web::HttpResponse, HttpError> {
  // generate a random filename
  let filename = uuid::Uuid::new_v4().to_string();
  let filepath = format!("/tmp/{filename}");
  // File::create is blocking operation, use threadpool
  let file_path_ptr = filepath.clone();
  let mut f = File::create(&file_path_ptr)
    .await
    .map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error while creating the file {err}"),
    })?;
  while let Some(bytes) = payload.next().await {
    let bytes = bytes.map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error while payload: {err}"),
    })?;
    f.write_all(&bytes).await.map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error while writing the file {err}"),
    })?;
  }
  f.shutdown().await.map_err(|err| HttpError {
    status: http::StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("Error while closing the file {err}"),
  })?;
  drop(f);
  let file = File::open(&file_path_ptr).await.map_err(|err| HttpError {
    status: http::StatusCode::INTERNAL_SERVER_ERROR,
    msg: format!("Error while opening the file {err}"),
  })?;
  // sending the file to the docker api
  let byte_stream =
    codec::FramedRead::new(file, codec::BytesCodec::new()).map(|r| {
      let bytes = r?.freeze();
      Ok::<_, std::io::Error>(bytes)
    });
  let quiet = query.quiet.unwrap_or(false);
  let body = hyper::Body::wrap_stream(byte_stream);
  let options = ImportImageOptions { quiet };
  let mut stream = state.docker_api.import_image(options, body, None);
  while let Some(res) = stream.next().await {
    let _ = res.map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("Error while importing the image {err}"),
    })?;
  }
  if let Err(err) = tokio::fs::remove_file(&filepath).await {
    log::warn!("Error while deleting the file {filepath}: {err}");
  }
  Ok(web::HttpResponse::Ok().into())
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_cargo_image);
  config.service(create_cargo_image);
  config.service(delete_cargo_image);
  config.service(inspect_cargo_image);
  config.service(import_cargo_image);
}

/// Cargo image unit tests
#[cfg(test)]
pub mod tests {

  use ntex::http;
  use tokio_util::codec;
  use ntex::http::client::ClientResponse;
  use futures::{StreamExt, TryStreamExt};
  use bollard_next::service::ImageInspect;

  use nanocl_stubs::generic::GenericDelete;
  use nanocl_stubs::cargo_image::CargoImagePartial;

  use crate::utils::tests::*;

  const ENDPOINT: &str = "/cargoes/images";

  /// Test utils to list cargo images
  pub async fn list(client: &TestClient) -> ClientResponse {
    client.send_get(ENDPOINT, None::<String>).await
  }

  /// Test utils to create cargo image
  pub async fn create(
    client: &TestClient,
    payload: &CargoImagePartial,
  ) -> ClientResponse {
    client
      .send_post(ENDPOINT, Some(payload), None::<String>)
      .await
  }

  /// Test utils to inspect cargo image
  pub async fn inspect(
    client: &TestClient,
    id_or_name: &str,
  ) -> ClientResponse {
    client
      .send_get(&format!("{ENDPOINT}/{id_or_name}"), None::<String>)
      .await
  }

  /// Test utils to delete cargo image
  pub async fn delete(client: &TestClient, id_or_name: &str) -> ClientResponse {
    client
      .send_delete(&format!("{ENDPOINT}/{id_or_name}"), None::<String>)
      .await
  }

  /// Test utils to ensure the cargo image exists
  pub async fn ensure_test_image() {
    let client = gen_default_test_client().await;
    let image = CargoImagePartial {
      name: "ghcr.io/nxthat/nanocl-get-started:latest".to_owned(),
    };
    let res = create(&client, &image).await;
    let mut stream = res.into_stream();
    while let Some(chunk) = stream.next().await {
      if let Err(err) = chunk {
        panic!("Error while creating test cargo image {err}");
      }
    }
  }

  /// Basic test to list cargo images
  #[ntex::test]
  async fn basic_list() {
    let client = gen_default_test_client().await;
    let resp = list(&client).await;
    let status = resp.status();
    test_status_code!(status, http::StatusCode::OK, "basic cargo image list");
  }

  /// Test to upload a cargo image as tarball
  /// Fail in the CI, need to investigate
  /// It works locally though but timeout in the CI
  #[ntex::test]
  async fn upload_tarball() {
    let client = gen_default_test_client().await;
    let curr_path = std::env::current_dir().unwrap();
    let filepath =
      std::path::Path::new(&curr_path).join("../../tests/busybox.tar.gz");
    let file = tokio::fs::File::open(&filepath)
      .await
      .expect("Open file for upload tarball failed");
    let byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new())
      .map(|r| {
        let bytes = ntex::util::Bytes::from(r?.freeze().to_vec());
        Ok::<_, std::io::Error>(bytes)
      });
    client
      .post(&format!("{ENDPOINT}/import"))
      .send_stream(byte_stream)
      .await
      .expect("Upload tarball failed");
  }

  /// Basic test to create cargo image with wrong name
  #[ntex::test]
  async fn basic_create_wrong_name() {
    let client = gen_default_test_client().await;
    let payload = CargoImagePartial {
      name: "test".to_owned(),
    };
    let resp = create(&client, &payload).await;
    let status = resp.status();
    test_status_code!(
      status,
      http::StatusCode::BAD_REQUEST,
      "basic cargo image create wrong name"
    );
  }

  /// Basic test to create, inspect and delete a cargo image
  #[ntex::test]
  async fn basic() {
    const TEST_IMAGE: &str = "busybox:unstable-musl";
    let client = gen_default_test_client().await;
    // Create
    let payload = CargoImagePartial {
      name: TEST_IMAGE.to_owned(),
    };
    let res = create(&client, &payload).await;
    let status = res.status();
    test_status_code!(status, http::StatusCode::OK, "cargo image create");
    let content_type = res
      .header("content-type")
      .expect("Expect create response to have content type header")
      .to_str()
      .unwrap();
    assert_eq!(
      content_type, "application/vdn.nanocl.raw-stream",
      "Expect content type to be application/vdn.nanocl.raw-stream got {content_type}"
    );
    let mut stream = res.into_stream();
    while let Some(chunk) = stream.next().await {
      if let Err(err) = chunk {
        panic!("Error while creating image {}", &err);
      }
    }
    // Inspect
    let mut res = inspect(&client, TEST_IMAGE).await;
    let status = res.status();
    test_status_code!(status, http::StatusCode::OK, "basic inspect image");
    let _body: ImageInspect = res
      .json()
      .await
      .expect("Expect inspect to return ImageInspect json data");
    // Delete
    let mut res = delete(&client, TEST_IMAGE).await;
    let status = res.status();
    test_status_code!(status, http::StatusCode::OK, "basic delete image");
    let body: GenericDelete = res
      .json()
      .await
      .expect("Expect delete to return GenericDelete json data");
    assert_eq!(
      body.count, 1,
      "Expect delete to return count 1 got {}",
      body.count
    );
  }
}
