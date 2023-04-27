use thiserror::Error;
use serde::{Serialize, Deserialize};
use ntex::http::{
  StatusCode,
  error::PayloadError,
  client::ClientResponse,
  client::error::{SendRequestError as NtexSendRequestError, JsonPayloadError},
};

use nanocl_utils::io_error::{FromIo, IoError};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponseError {
  pub msg: String,
}

#[derive(Debug, Error)]
pub struct ApiError {
  pub status: StatusCode,
  pub msg: String,
}

impl std::fmt::Display for ApiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", &self.msg)
  }
}

#[derive(Debug, Error)]
pub struct SendRequestError {
  pub msg: String,
}

impl From<NtexSendRequestError> for NanocldClientError {
  fn from(err: NtexSendRequestError) -> Self {
    NanocldClientError::SendRequestError(SendRequestError {
      msg: err.to_string(),
    })
  }
}

impl std::fmt::Display for SendRequestError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", &self.msg)
  }
}

#[derive(Debug, Error)]
pub enum NanocldClientError {
  #[error(transparent)]
  Api(#[from] ApiError),
  #[error(transparent)]
  Payload(#[from] PayloadError),
  #[error(transparent)]
  SendRequestError(#[from] SendRequestError),
  #[error(transparent)]
  JsonPayload(#[from] JsonPayloadError),
  #[error(transparent)]
  SerdeUrlEncode(#[from] serde_urlencoded::ser::Error),
  #[error(transparent)]
  Utf8Error(#[from] std::string::FromUtf8Error),
  #[error(transparent)]
  WsClientBuilderError(#[from] ntex::ws::error::WsClientBuilderError),
  #[error(transparent)]
  WsClientError(#[from] ntex::ws::error::WsClientError),
}

impl FromIo<Box<IoError>> for NanocldClientError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    let inner = match self {
      NanocldClientError::Api(err) => {
        std::io::Error::new(std::io::ErrorKind::Other, err)
      }
      NanocldClientError::JsonPayload(err) => {
        std::io::Error::new(std::io::ErrorKind::InvalidData, err)
      }
      _ => std::io::Error::new(std::io::ErrorKind::Other, self),
    };
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner,
    })
  }
}

pub(crate) async fn is_api_error(
  res: &mut ClientResponse,
  status: &StatusCode,
) -> Result<(), NanocldClientError> {
  if status.is_server_error() || status.is_client_error() {
    let err = res.json::<ApiResponseError>().await?;
    return Err(NanocldClientError::Api(ApiError {
      status: *status,
      msg: err.msg,
    }));
  }
  Ok(())
}
