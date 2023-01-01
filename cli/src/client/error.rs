use thiserror::Error;
use serde::{Serialize, Deserialize};
use ntex::http::{
  StatusCode,
  error::PayloadError,
  client::{
    ClientResponse,
    error::{SendRequestError, JsonPayloadError},
  },
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponseError {
  pub msg: String,
}

#[derive(Debug, Error)]
pub struct ApiError {
  pub(crate) status: StatusCode,
  pub(crate) msg: String,
}

impl std::fmt::Display for ApiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", &self.msg)
  }
}

#[derive(Debug, Error)]
pub enum NanocldError {
  #[error(transparent)]
  Api(#[from] ApiError),
  #[error(transparent)]
  Payload(#[from] PayloadError),
  #[error(transparent)]
  SendRequest(#[from] SendRequestError),
  #[error(transparent)]
  JsonPayload(#[from] JsonPayloadError),
  #[error(transparent)]
  SerdeUrlEncode(#[from] serde_urlencoded::ser::Error),
  #[error(transparent)]
  Utf8Error(#[from] std::string::FromUtf8Error),
}

pub async fn is_api_error(
  res: &mut ClientResponse,
  status: &StatusCode,
) -> Result<(), NanocldError> {
  if status.is_server_error() || status.is_client_error() {
    let err = res.json::<ApiResponseError>().await?;
    return Err(NanocldError::Api(ApiError {
      status: *status,
      msg: err.msg,
    }));
  }
  Ok(())
}
