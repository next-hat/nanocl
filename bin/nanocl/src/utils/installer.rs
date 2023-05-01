use ntex::http;

use nanocl_utils::io_error::{FromIo, IoResult};
use nanocl_utils::http_error::HttpError;
use nanocl_utils::http_client_error::HttpClientError;

/// Get template from our GitHub repo
async fn get() -> Result<String, HttpClientError> {
  let client = http::client::Client::new();

  let mut res = client
    .get("https://raw.githubusercontent.com/nxthat/nanocl/0450e4f4066ef16b172d065e09412fba9ee49376/installer.nightly.yml")
    .send().await
    .map_err(|err| err
      .map_err_context(|| "Unable to fetch installer template"))?;

  let status = res.status();
  if status.is_server_error() || status.is_client_error() {
    return Err(HttpClientError::HttpError(HttpError {
      status,
      msg: "Unable to fetch installer template".into(),
    }));
  }

  let body = res.body().await.map_err(|err| {
    err.map_err_context(|| "Unable to fetch installer template")
  })?;

  let body = String::from_utf8(body.to_vec()).map_err(|err| {
    err.map_err_context(|| "Unable to fetch installer template")
  })?;

  Ok(body)
}

/// Get template from our GitHub repo or from the specified file if it's provided
pub async fn get_template(template: Option<String>) -> IoResult<String> {
  match template {
    None => {
      let template = get().await?;
      Ok(template)
    }
    Some(template) => {
      let template = std::fs::read_to_string(std::path::Path::new(&template))
        .map_err(|err| {
        err.map_err_context(|| "Unable to read installer template")
      })?;
      Ok(template)
    }
  }
}
