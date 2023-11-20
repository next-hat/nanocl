use ntex::web;
use ntex::http;

/// An http response error
#[derive(Clone, Debug)]
pub struct HttpError {
  pub msg: String,
  pub status: http::StatusCode,
}

pub type HttpResult<T, E = HttpError> = Result<T, E>;

impl HttpError {
  /// Create a new HttpError
  pub fn new<T>(status: http::StatusCode, msg: T) -> Self
  where
    T: ToString,
  {
    Self {
      status,
      msg: msg.to_string(),
    }
  }

  /// Create a new HttpError with status BadRequest - 400
  pub fn bad_request<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::BAD_REQUEST, msg)
  }

  /// Create a new HttpError with status Unauthorized - 401
  pub fn unauthorized<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::UNAUTHORIZED, msg)
  }

  pub fn forbidden<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::FORBIDDEN, msg)
  }

  /// Create a new HttpError with status NotFound - 404
  pub fn not_found<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::NOT_FOUND, msg)
  }

  /// Create a new HttpError with status InternalServerError - 500
  pub fn internal_server_error<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::INTERNAL_SERVER_ERROR, msg)
  }

  /// Create a new HttpError with status Conflict - 409
  pub fn conflict<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::CONFLICT, msg)
  }

  pub fn bad_gateway<T>(msg: T) -> Self
  where
    T: ToString,
  {
    Self::new(http::StatusCode::BAD_GATEWAY, msg)
  }
}

/// Helper function to display an HttpError
impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", self.status, self.msg)
  }
}

/// Implement standard error for HttpError
impl std::error::Error for HttpError {}

/// Helper function to convert an HttpError into a ntex::web::HttpResponse
impl web::WebResponseError for HttpError {
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    let err_json = serde_json::json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}

#[cfg(feature = "io")]
impl From<crate::io::IoError> for HttpError {
  fn from(err: crate::io::IoError) -> Self {
    match err.inner.kind() {
      std::io::ErrorKind::NotFound => HttpError::not_found(err.to_string()),
      std::io::ErrorKind::AlreadyExists => HttpError::conflict(err.to_string()),
      std::io::ErrorKind::ConnectionAborted => {
        HttpError::bad_gateway(err.to_string())
      }
      std::io::ErrorKind::ConnectionRefused => {
        HttpError::bad_gateway(err.to_string())
      }
      std::io::ErrorKind::InvalidData => {
        HttpError::bad_request(err.to_string())
      }
      _ => HttpError::internal_server_error(err.to_string()),
    }
  }
}

#[cfg(feature = "io")]
impl From<Box<crate::io::IoError>> for HttpError {
  fn from(err: Box<crate::io::IoError>) -> Self {
    (*err).into()
  }
}

#[cfg(feature = "bollard")]
impl From<bollard_next::errors::Error> for HttpError {
  fn from(err: bollard_next::errors::Error) -> Self {
    match err {
      bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message,
      } => HttpError {
        msg: message,
        status: http::StatusCode::from_u16(status_code)
          .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR),
      },
      _ => HttpError {
        msg: format!("{err}"),
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
      },
    }
  }
}

#[cfg(feature = "io")]
impl crate::io::FromIo<HttpError> for HttpError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> HttpError
  where
    C: ToString + std::fmt::Display,
  {
    HttpError::new(self.status, format!("{}: {}", context(), self.msg))
  }
}
