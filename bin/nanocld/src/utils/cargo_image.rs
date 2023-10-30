use ntex::http;
use ntex::util::Bytes;
use futures::StreamExt;

use bollard_next::service::CreateImageInfo;
use bollard_next::models::{ImageInspect, ImageSummary};

use nanocl_error::http::HttpError;
use nanocl_stubs::generic::GenericDelete;

use crate::models::DaemonState;

use super::stream;

/// ## Parse image info
///
/// Get the image name and tag from a string
///
/// ## Arguments
///
/// * [image_info](str) The string to parse
///
/// ## Returns
///
/// * [Result](Result) The result of the operation
///   * [Ok]((String, String)) - The image name and tag
///   * [Err](HttpError) - An http response error if something went wrong
///
pub fn parse_image_info(
  image_info: &str,
) -> Result<(String, String), HttpError> {
  let image_info: Vec<&str> = image_info.split(':').collect();

  if image_info.len() != 2 {
    return Err(HttpError {
      msg: String::from("missing tag in image name"),
      status: http::StatusCode::BAD_REQUEST,
    });
  }

  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// ## List
///
/// List all cargo images installed
///
/// ## Arguments
/// * [docker_api](bollard_next::Docker) docker api client
///
/// ## Returns
/// * [Result](Result) - The result of the operation
///   * [Ok](Vec<ImageSummary>) - A list of image summary
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn list(
  opts: &bollard_next::image::ListImagesOptions<String>,
  state: &DaemonState,
) -> Result<Vec<ImageSummary>, HttpError> {
  let items = state.docker_api.list_images(Some(opts.clone())).await?;

  Ok(items)
}

/// ## Inspect by name
///
/// Get detailed information on a cargo image by name
///
/// ## Arguments
///
/// * [image_name](str) name of the image to inspect
/// * [docker_api](bollard_next::Docker) docker api client
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](ImageInspect) - Image inspect
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn inspect_by_name(
  image_name: &str,
  state: &DaemonState,
) -> Result<ImageInspect, HttpError> {
  let image = state.docker_api.inspect_image(image_name).await?;

  Ok(image)
}

/// ## Pull
///
/// Pull a cargo/container image from the docker registry by name and tag
///
/// ## Arguments
///
/// * [image_name](str) name of the image to download
/// * [tag](str) tag of the image to download
/// * [docker_api](bollard_next::Docker) docker api client
///
/// ## Returns
///
/// * [Result](Result) The result of the operation
///   * [Ok](Receiver<Result<Bytes, web::error::Error>>) - A stream of bytes
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn pull(
  image_name: &str,
  tag: &str,
  state: &DaemonState,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
  let from_image = image_name.to_owned();
  let tag = tag.to_owned();
  let docker_api = state.docker_api.clone();

  let stream = docker_api.create_image(
    Some(bollard_next::image::CreateImageOptions {
      from_image,
      tag,
      ..Default::default()
    }),
    None,
    None,
  );
  let stream =
    stream::transform_stream::<CreateImageInfo, CreateImageInfo>(stream);
  Ok(stream)
}

/// ## Delete
///
/// Delete an installed cargo/container image by id or name
///
/// ## Arguments
///
/// * [image_name](str) name of the image to delete
/// * [docker_api](bollard_next::Docker) docker api client
///
/// ## Returns
///
/// * [Result](Result) The result of the operation
///   * [Ok](GenericDelete) - A generic delete response
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn delete(
  id_or_name: &str,
  state: &DaemonState,
) -> Result<GenericDelete, HttpError> {
  state
    .docker_api
    .remove_image(id_or_name, None, None)
    .await?;
  let res = GenericDelete { count: 1 };
  Ok(res)
}
