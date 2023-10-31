use ntex::http;

use nanocl_error::io::{FromIo, IoResult};
use nanocl_error::http::HttpError;
use nanocl_error::http_client::HttpClientError;

use crate::version::{VERSION, CHANNEL};

/// ## Get
///
/// Get template from our GitHub repo for installation
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](String) The template
///   * [Err](HttpClientError) An error occured
///
async fn get() -> Result<String, HttpClientError> {
  let client = http::client::Client::new();
  let url = format!("https://raw.githubusercontent.com/nxthat/nanocl/release/{CHANNEL}/bin/nanocl/{VERSION}/installer.yml");
  let mut res = client.get(url).send().await.map_err(|err| {
    err.map_err_context(|| "Unable to fetch installer template")
  })?;
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

/// ## Get template
///
/// Get template from our GitHub repo or from the specified file if it's provided
///
/// ## Arguments
///
/// * [template](Option<String>) The template file
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](String) The template
///   * [Err](nanocl_error::io::IoError) An error occured
///
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
