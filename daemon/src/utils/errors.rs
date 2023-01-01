use ntex::http::StatusCode;

use crate::errors::HttpResponseError;

pub fn docker_error_ref(err: &bollard::errors::Error) -> HttpResponseError {
  match err {
    bollard::errors::Error::DockerResponseServerError {
      status_code,
      message,
    } => HttpResponseError {
      msg: message.to_owned(),
      status: StatusCode::from_u16(status_code.to_owned()).unwrap(),
    },
    bollard::errors::Error::JsonDataError { message, .. } => {
      HttpResponseError {
        msg: message.to_owned(),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      }
    }
    _ => HttpResponseError {
      msg: format!("unexpected docker api error {:#?}", err),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    },
  }
}
