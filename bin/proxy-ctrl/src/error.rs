use ntex::web;
use ntex::http::StatusCode;
use serde_json::json;

pub(crate) enum ErrorHint {
  Warning(String),
  Error(String),
}

impl std::fmt::Display for ErrorHint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ErrorHint::Warning(msg) => write!(f, "{msg}"),
      ErrorHint::Error(msg) => write!(f, "{msg}"),
    }
  }
}

impl ErrorHint {
  pub(crate) fn warning(msg: String) -> ErrorHint {
    ErrorHint::Warning(msg)
  }

  pub(crate) fn error(msg: String) -> ErrorHint {
    ErrorHint::Error(msg)
  }

  pub(crate) fn print(&self) {
    match self {
      ErrorHint::Warning(msg) => log::warn!("{msg}"),
      ErrorHint::Error(msg) => log::error!("{msg}"),
    }
  }
}

impl From<ErrorHint> for HttpError {
  fn from(hint: ErrorHint) -> Self {
    match hint {
      ErrorHint::Warning(msg) => HttpError {
        msg,
        status: StatusCode::BAD_REQUEST,
      },
      ErrorHint::Error(msg) => HttpError {
        msg,
        status: StatusCode::INTERNAL_SERVER_ERROR,
      },
    }
  }
}

#[derive(Clone, Debug)]
pub struct HttpError {
  pub msg: String,
  pub status: StatusCode,
}

impl std::fmt::Display for HttpError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}]: {}", &self.status, &self.msg)
  }
}

impl web::WebResponseError for HttpError {
  // builds the actual response to send back when an error occurs
  fn error_response(&self, _: &web::HttpRequest) -> web::HttpResponse {
    log::error!("{self}");
    let err_json = json!({ "msg": self.msg });
    web::HttpResponse::build(self.status).json(&err_json)
  }
}

#[cfg(test)]
mod tests {

  use super::*;

  use crate::utils::tests::*;

  #[test]
  fn error_hint_basic() {
    before();
    let hint = ErrorHint::warning("This is a warning".to_string());
    println!("{hint}");
    hint.print();
    assert_eq!(hint.to_string(), "This is a warning");
    let hint = ErrorHint::error("This is an error".to_string());
    println!("{hint}");
    assert_eq!(hint.to_string(), "This is an error");
    hint.print();
  }
}
