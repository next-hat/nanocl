use ntex::web;
use ntex::http::StatusCode;
use serde_json::json;
use thiserror::Error;

use bollard::errors::Error as DockerError;
#[cfg(feature = "dev")]
use utoipa::ToSchema;

/// Http response error
#[derive(Debug, Error)]
pub struct HttpResponseError {
  pub(crate) msg: String,
  pub(crate) status: StatusCode,
}

impl From<DockerError> for HttpResponseError {
  fn from(err: DockerError) -> Self {
    match err {
      DockerError::DockerResponseServerError {
        status_code,
        message,
      } => HttpResponseError {
        msg: message,
        status: StatusCode::from_u16(status_code)
          .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
      },
      _ => HttpResponseError {
        msg: format!("{}", err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      },
    }
  }
}

impl std::fmt::Display for HttpResponseError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", self.status, self.msg)
  }
}

pub trait IntoHttpResponseError {
  fn to_http_error(&self) -> HttpResponseError;
}

impl web::WebResponseError for HttpResponseError {
  // builds the actual response to send back when an error occurs
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    log::error!("Error response: {}", self);
    let err_json = json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}

/// Api Error Structure that server send to client
/// Used to generate open api specification
#[cfg(feature = "dev")]
#[cfg_attr(feature = "dev", derive(ToSchema))]
#[allow(dead_code)]
pub struct ApiError {
  pub(crate) msg: String,
}

/// Generic Daemon error
#[derive(Debug, Error)]
pub enum DaemonError {
  /// Generic system io error
  #[error(transparent)]
  Io(#[from] std::io::Error),
  /// Yaml parsing error
  #[error(transparent)]
  Yaml(#[from] serde_yaml::Error),
  /// Docker api error
  #[error(transparent)]
  Docker(#[from] DockerError),
  /// Diesel migration error
  #[error(transparent)]
  DieselMigration(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
  /// HttpResponseError
  #[error(transparent)]
  HttpResponse(#[from] HttpResponseError),
}

pub fn parse_main_error(err: DaemonError) -> i32 {
  match err {
    DaemonError::Docker(err) => match err {
      bollard::errors::Error::HyperResponseError { err } => {
        if err.is_connect() {
          log::error!("unable to connect to docker host {err}");
          return 1;
        }
        log::error!("{}", err);
        1
      }
      _ => {
        log::error!("{}", err);
        1
      }
    },
    _ => {
      log::error!("{}", err);
      1
    }
  }
}
