use nanocl_error::http::{HttpError, HttpResult};

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
