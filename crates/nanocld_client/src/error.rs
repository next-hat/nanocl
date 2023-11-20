use ntex::http;

use nanocl_error::io::FromIo;
use nanocl_error::http::HttpError;
use nanocl_error::http_client::{HttpClientResult, HttpClientError};

pub(crate) async fn is_api_error(
  res: &mut http::client::ClientResponse,
  status: &http::StatusCode,
) -> HttpClientResult<()> {
  if status.is_server_error() || status.is_client_error() {
    let err = res.json::<serde_json::Value>().await.map_err(|err| {
      err.map_err_context(|| "Unable to serialize error response")
    })?;
    let default = serde_json::Value::String("".to_owned());
    let msg = err
      .get("msg")
      .unwrap_or(&default)
      .as_str()
      .unwrap_or_default();
    return Err(HttpClientError::HttpError(HttpError {
      status: *status,
      msg: msg.to_owned(),
    }));
  }
  Ok(())
}
