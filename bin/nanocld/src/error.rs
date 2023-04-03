use ntex::web;
use ntex::http::StatusCode;

use thiserror::Error;
use serde_json::json;
use bollard_next::errors::Error as DockerError;

/// Cli Error
#[derive(Debug)]
pub struct CliError {
  pub(crate) code: i32,
  pub(crate) msg: String,
}

impl CliError {
  pub fn new<T>(code: i32, msg: T) -> Self
  where
    T: Into<String>,
  {
    Self {
      code,
      msg: msg.into(),
    }
  }
}

/// Http response error
#[derive(Clone, Debug, Error)]
pub struct HttpError {
  pub(crate) msg: String,
  pub(crate) status: StatusCode,
}

impl From<DockerError> for HttpError {
  fn from(err: DockerError) -> Self {
    match err {
      DockerError::DockerResponseServerError {
        status_code,
        message,
      } => HttpError {
        msg: message,
        status: StatusCode::from_u16(status_code)
          .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
      },
      _ => HttpError {
        msg: format!("{err}"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      },
    }
  }
}

impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", self.status, self.msg)
  }
}

impl web::WebResponseError for HttpError {
  // builds the actual response to send back when an error occurs
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    log::error!("[{}] error: {}", self.status, self.msg);
    let err_json = json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}

impl From<HttpError> for CliError {
  fn from(err: HttpError) -> Self {
    Self {
      code: 1,
      msg: err.msg,
    }
  }
}

impl From<DockerError> for CliError {
  fn from(err: DockerError) -> Self {
    Self {
      code: 1,
      msg: format!("{err}"),
    }
  }
}
