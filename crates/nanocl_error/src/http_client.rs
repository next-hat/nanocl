use super::io::{IoError, FromIo};
use super::http::HttpError;

#[derive(Debug)]
pub enum HttpClientError {
  IoError(IoError),
  HttpError(HttpError),
}

pub type HttpClientResult<T> = Result<T, HttpClientError>;

impl std::fmt::Display for HttpClientError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      HttpClientError::IoError(err) => write!(f, "{err}"),
      HttpClientError::HttpError(err) => write!(f, "{err}"),
    }
  }
}

impl std::error::Error for HttpClientError {}

impl From<HttpClientError> for IoError {
  fn from(f: HttpClientError) -> Self {
    match f {
      HttpClientError::IoError(err) => err,
      HttpClientError::HttpError(err) => err.into(),
    }
  }
}

impl From<Box<HttpClientError>> for IoError {
  fn from(f: Box<HttpClientError>) -> Self {
    match *f {
      HttpClientError::IoError(err) => err,
      HttpClientError::HttpError(err) => err.into(),
    }
  }
}

impl From<HttpClientError> for Box<IoError> {
  fn from(f: HttpClientError) -> Self {
    match f {
      HttpClientError::IoError(err) => Box::new(err),
      HttpClientError::HttpError(err) => Box::new(err.into()),
    }
  }
}

impl From<Box<IoError>> for HttpClientError {
  fn from(f: Box<IoError>) -> Self {
    Self::IoError(*f)
  }
}

impl From<HttpError> for HttpClientError {
  fn from(f: HttpError) -> Self {
    Self::HttpError(f)
  }
}

impl From<HttpClientError> for HttpError {
  fn from(f: HttpClientError) -> Self {
    match f {
      HttpClientError::IoError(err) => err.into(),
      HttpClientError::HttpError(err) => err,
    }
  }
}

impl From<Box<HttpClientError>> for HttpClientError {
  fn from(f: Box<HttpClientError>) -> Self {
    *f
  }
}

impl FromIo<Box<HttpClientError>> for HttpClientError {
  fn map_err_context<C>(
    self,
    context: impl FnOnce() -> C,
  ) -> Box<HttpClientError>
  where
    C: ToString + std::fmt::Display,
  {
    match self {
      HttpClientError::IoError(err) => {
        Box::new(HttpClientError::IoError(err.map_err_context(context)))
      }
      HttpClientError::HttpError(err) => {
        Box::new(HttpClientError::HttpError(err.map_err_context(context)))
      }
    }
  }
}
