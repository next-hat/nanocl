use thiserror::Error;

use crate::client::error::NanocldError;

#[derive(Debug, Error)]
pub enum CliError {
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  Parse(#[from] serde_yaml::Error),
  #[error(transparent)]
  Client(#[from] NanocldError),
}
