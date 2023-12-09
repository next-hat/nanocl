use ntex::util::Bytes;
use futures::StreamExt;

use bollard_next::{
  service::CreateImageInfo,
  models::{ImageInspect, ImageSummary},
};

use nanocl_error::http::{HttpError, HttpResult};

use crate::models::DaemonState;

use super::stream;

/// Get the image name and tag from a string
pub(crate) fn parse_image_name(name: &str) -> HttpResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
  if image_info.len() != 2 {
    return Err(HttpError::bad_request("Missing tag in image name"));
  }
  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// List all cargo images installed
pub(crate) async fn list(
  opts: &bollard_next::image::ListImagesOptions<String>,
  state: &DaemonState,
) -> HttpResult<Vec<ImageSummary>> {
  let items = state.docker_api.list_images(Some(opts.clone())).await?;
  Ok(items)
}

/// Get detailed information on a cargo image by name
pub(crate) async fn inspect_by_name(
  image_name: &str,
  state: &DaemonState,
) -> HttpResult<ImageInspect> {
  let image = state.docker_api.inspect_image(image_name).await?;
  Ok(image)
}

/// Pull a cargo/container image from the docker registry by name and tag
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

/// Delete an installed cargo/container image by id or name
pub(crate) async fn delete(
  id_or_name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  state
    .docker_api
    .remove_image(id_or_name, None, None)
    .await?;
  Ok(())
}
