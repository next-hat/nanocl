use thiserror::Error;

use nanocld_client::error::{ApiError, NanocldClientError};

#[derive(Debug, Error)]
pub enum CliError {
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  ParseYml(#[from] serde_yaml::Error),
  #[error(transparent)]
  Client(#[from] NanocldClientError),
  #[error(transparent)]
  Docker(#[from] bollard_next::errors::Error),
  #[error(transparent)]
  ParseJson(#[from] serde_json::Error),
  #[error(transparent)]
  Api(#[from] ApiError),
  #[error("{msg}")]
  Custom { msg: String },
}

impl CliError {
  pub fn exit(&self) {
    match self {
      CliError::Client(err) => match err {
        NanocldClientError::Api(err) => {
          eprintln!("Daemon [{}]: {}", err.status, err.msg);
        }
        _ => eprintln!("{err}"),
      },
      _ => eprintln!("{self}"),
    }
    std::process::exit(1);
  }
}
