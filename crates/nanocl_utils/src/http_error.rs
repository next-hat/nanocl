use ntex::web;
use ntex::http;

/// An http response error
#[derive(Clone, Debug)]
pub struct HttpError {
  pub msg: String,
  pub status: http::StatusCode,
}

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
}

/// Helper function to display an HttpError
impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", self.status, self.msg)
  }
}

/// Helper function to convert an HttpError into a ntex::web::HttpResponse
impl web::WebResponseError for HttpError {
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    log::debug!("Replying error: [{}] {}", self.status, self.msg);
    let err_json = serde_json::json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}

#[cfg(feature = "io_error")]
impl From<crate::io_error::IoError> for HttpError {
  fn from(err: crate::io_error::IoError) -> Self {
    match err.inner.kind() {
      std::io::ErrorKind::NotFound => HttpError::not_found(err.to_string()),
      _ => HttpError::internal_server_error(err.to_string()),
    }
  }
}
