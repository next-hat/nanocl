use ntex::util::Bytes;
use futures::StreamExt;

use bollard_next::service::CreateImageInfo;

use nanocl_error::http::{HttpError, HttpResult};

use crate::models::SystemState;

use super::stream;

/// Get the image name and tag from a string
pub fn parse_name(name: &str) -> HttpResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
  if image_info.len() != 2 {
    return Err(HttpError::bad_request("Missing tag in image name"));
  }
  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// Pull a cargo/container image from the docker registry by name and tag
pub async fn pull(
  image_name: &str,
  tag: &str,
  state: &SystemState,
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
