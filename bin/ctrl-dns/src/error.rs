use ntex::web;
use ntex::http::StatusCode;

/// Error hint in case of an error
pub enum ErrorHint {
  /// Critical process exited
  Error(String),
  /// Warning just for information
  Warning(String),
}

/// Display the ErrorHint
impl std::fmt::Display for ErrorHint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ErrorHint::Error(msg) => write!(f, "[ERROR]: {msg}"),
      ErrorHint::Warning(msg) => write!(f, "[WARNING]: {msg}"),
    }
  }
}

#[derive(Debug)]
pub struct HttpError {
  pub msg: String,
  pub status: StatusCode,
}

impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", &self.status, &self.msg)
  }
}

impl std::error::Error for HttpError {}

impl web::WebResponseError for HttpError {
  // builds the actual response to send back when an error occurs
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    let err_json = serde_json::json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}
