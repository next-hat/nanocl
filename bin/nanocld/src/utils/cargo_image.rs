use ntex::http;
use ntex::util::Bytes;
use futures::StreamExt;

use bollard_next::service::CreateImageInfo;
use bollard_next::models::{ImageInspect, ImageSummary};

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::generic::GenericDelete;

use crate::models::DaemonState;

use super::stream;

/// ## Parse image info
///
/// Get the image name and tag from a string
///
/// ## Arguments
///
/// * [image](str) The string to parse
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a tuple of ([String](String), [image_tag](String))
///
pub(crate) fn parse_image_name(name: &str) -> HttpResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
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
///
/// * [opts](bollard_next::image::ListImagesOptions) - The list options
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [ImageSummary](ImageSummary)
///
pub(crate) async fn list(
  opts: &bollard_next::image::ListImagesOptions<String>,
  state: &DaemonState,
) -> HttpResult<Vec<ImageSummary>> {
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
/// ## Return
///
/// [HttpResult](HttpResult) containing a [ImageInspect](ImageInspect)
///
pub(crate) async fn inspect_by_name(
  image_name: &str,
  state: &DaemonState,
) -> HttpResult<ImageInspect> {
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
/// ## Return
///
/// [HttpResult](HttpResult) containing a [StreamExt](StreamExt) of [CreateImageInfo](CreateImageInfo)
///
pub(crate) async fn pull(
  image_name: &str,
  tag: &str,
  state: &DaemonState,
) -> HttpResult<impl StreamExt<Item = HttpResult<Bytes>>> {
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
/// ## Return
///
/// [HttpResult](HttpResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete(
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
