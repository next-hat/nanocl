use ntex::rt;
use ntex::web;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use ntex::http::StatusCode;
use futures::StreamExt;
use bollard::Docker;
use bollard::models::{ImageInspect, ImageSummary};

use nanocl_stubs::generic::GenericDelete;

use crate::error::HttpResponseError;

/// ## List cargo image
///
/// List all cargo images installed
///
/// ## Arguments
/// - [docker_api](bollard::Docker) docker api client
///
/// ## Return
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ImageSummary>) - A list of image summary
///   - [Err](HttpResponseError) - An http response error if something went wrong
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
/// use crate::utils::cargo_image;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = cargo_image::list(&docker_api).await;
/// ```
///
pub async fn list(
  docker_api: &Docker,
  opts: bollard::image::ListImagesOptions<String>,
) -> Result<Vec<ImageSummary>, HttpResponseError> {
  let items = docker_api.list_images(Some(opts)).await?;

  Ok(items)
}

/// # Inspect cargo image
///
/// Get detailed information on a cargo image
///
/// ## Arguments
///
/// - [image_name](str) name of the image to inspect
/// - [docker_api](bollard::Docker) docker api client
///
/// ## Return
///
/// - [Result](Result) - The result of the operation
///   - [Ok](ImageInspect) - Image inspect
///   - [Err](HttpResponseError) - An http response error if something went wrong
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
/// use crate::utils::cargo_image;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = cargo_image::inspect("nginx:latest", &docker_api).await;
/// ```
///
pub async fn inspect(
  image_name: &str,
  docker_api: &Docker,
) -> Result<ImageInspect, HttpResponseError> {
  let image = docker_api.inspect_image(image_name).await?;

  Ok(image)
}

/// # Download cargo image
///
/// Download a cargo/container image from the docker registry
///
/// ## Arguments
///
/// - [image_name](str) name of the image to download
/// - [tag](str) tag of the image to download
/// - [docker_api](bollard::Docker) docker api client
///
///
/// ## Return
///
/// - [Result](Result) The result of the operation
///   - [Ok](Receiver<Result<Bytes, web::error::Error>>) - A stream of bytes
///   - [Err](HttpResponseError) - An http response error if something went wrong
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
/// use futures::StreamExt;
/// use crate::utils::cargo_image;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let stream = cargo_image::download("nginx:latest", &docker_api).await.unwrap();
/// while let Some(result) = stream.next().await {
///  // Do something with the result
/// }
/// ```
///
pub async fn download(
  from_image: &str,
  tag: &str,
  docker_api: &Docker,
) -> Result<mpsc::Receiver<Result<Bytes, web::error::Error>>, HttpResponseError>
{
  let from_image = from_image.to_owned();
  let tag = tag.to_owned();
  let docker_api = docker_api.to_owned();
  let (tx, rx_body) = mpsc::channel();

  rt::spawn(async move {
    let mut stream = docker_api.create_image(
      Some(bollard::image::CreateImageOptions {
        from_image,
        tag,
        ..Default::default()
      }),
      None,
      None,
    );
    while let Some(result) = stream.next().await {
      match result {
        Err(err) => {
          let err = ntex::web::Error::new(web::error::InternalError::default(
            format!("{err:?}"),
            StatusCode::INTERNAL_SERVER_ERROR,
          ));
          let _ = tx.send(Err::<_, web::error::Error>(err));
          break;
        }
        Ok(result) => {
          let data = match serde_json::to_string(&result) {
            Err(err) => {
              let err =
                ntex::web::Error::new(web::error::InternalError::default(
                  format!("{err:?}"),
                  StatusCode::INTERNAL_SERVER_ERROR,
                ));
              let _ = tx.send(Err::<_, web::error::Error>(err));
              break;
            }
            Ok(data) => data,
          };
          // Add the length of the data to the beginning of the stream
          // The length is an usize
          // The stream is terminated by a newline
          let len = data.len();
          let response = format!("{len}\n{data}\n");

          if tx
            .send(Ok::<_, web::error::Error>(Bytes::from(response)))
            .is_err()
          {
            // If the client is disconnected we stop the operation
            break;
          }
        }
      }
    }
    tx.close();
  });
  Ok(rx_body)
}

/// Delete cargo image
///
/// Delete an installed cargo/container image
///
/// ## Arguments
///
/// - [image_name](str) name of the image to delete
/// - [docker_api](bollard::Docker) docker api client
///
/// ## Return
///
/// - [Result](Result) The result of the operation
///   - [Ok](GenericDelete) - A generic delete response
///   - [Err](HttpResponseError) - An http response error if something went wrong
///
/// ## Example
///
/// ```rust,norun
/// use bollard::Docker;
/// use crate::utils::cargo_image;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = cargo_image::delete("nginx:latest", &docker_api).await;
/// ```
///
pub async fn delete(
  id_or_name: &str,
  docker_api: &Docker,
) -> Result<GenericDelete, HttpResponseError> {
  docker_api.remove_image(id_or_name, None, None).await?;
  let res = GenericDelete { count: 1 };

  Ok(res)
}

/// # Parse image info
///
/// Get the image name and tag from a string
///
/// ## Arguments
///
/// - [image_info](str) The string to parse
///
/// ## Return
///
/// - [Result](Result) The result of the operation
///   - [Ok](String, String) - The image name and tag
///   - [Err](HttpResponseError) - An http response error if something went wrong
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils::cargo_image;
///
/// let (name, tag) = cargo_image::parse_image_info("nginx:latest").unwrap();
/// ```
///
pub fn parse_image_info(
  image_info: &str,
) -> Result<(String, String), HttpResponseError> {
  let image_info: Vec<&str> = image_info.split(':').collect();

  if image_info.len() != 2 {
    return Err(HttpResponseError {
      msg: String::from("missing tag in image name"),
      status: StatusCode::BAD_REQUEST,
    });
  }

  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}
