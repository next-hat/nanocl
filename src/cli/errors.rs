use thiserror::Error;

use crate::client::error::{NanocldError, ApiError};

#[derive(Debug, Error)]
pub enum CliError {
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  Parse(#[from] serde_yaml::Error),
  #[error(transparent)]
  Client(#[from] NanocldError),
  #[error(transparent)]
  Docker(#[from] bollard::errors::Error),
  #[error(transparent)]
  Api(#[from] ApiError),
  #[error("{msg:?}")]
  Custom { msg: String },
}
