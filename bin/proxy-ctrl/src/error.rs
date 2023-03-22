use ntex::web;
use ntex::http::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub(crate) enum ErrorHint {
  Warning((i32, String)),
  Error((i32, String)),
}

impl std::fmt::Display for ErrorHint {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ErrorHint::Warning(msg) => write!(f, "[{}]: {}", &msg.0, &msg.1),
      ErrorHint::Error(msg) => write!(f, "[{}]: {}", &msg.0, &msg.1),
    }
  }
}

impl ErrorHint {
  pub(crate) fn warning(code: i32, msg: String) -> ErrorHint {
    ErrorHint::Warning((code, msg))
  }

  pub(crate) fn error(code: i32, msg: String) -> ErrorHint {
    ErrorHint::Error((code, msg))
  }

  pub(crate) fn print(&self) {
    match self {
      ErrorHint::Warning(_) => log::warn!("{self}"),
      ErrorHint::Error(_) => log::error!("{self}"),
    }
  }

  pub(crate) fn exit(&self) {
    self.print();
    match self {
      ErrorHint::Warning(w) => std::process::exit(w.0),
      ErrorHint::Error(e) => std::process::exit(e.0),
    }
  }
}

impl From<ErrorHint> for HttpError {
  fn from(hint: ErrorHint) -> Self {
    match hint {
      ErrorHint::Warning(err) => HttpError {
        msg: err.1,
        status: StatusCode::BAD_REQUEST,
      },
      ErrorHint::Error(err) => HttpError {
        msg: err.1,
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
    let hint = ErrorHint::warning(1, "This is a warning".to_string());
    println!("{hint}");
    hint.print();
    assert_eq!(hint.to_string(), "[1]: This is a warning");
    let hint = ErrorHint::error(2, "This is an error".to_string());
    println!("{hint}");
    assert_eq!(hint.to_string(), "[2]: This is an error");
    hint.print();
  }
}
