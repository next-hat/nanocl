use thiserror::Error;

use nanocl_client::error::{NanoclClientError, ApiError};

use crate::models::Cli;

#[derive(Debug, Error)]
pub enum CliError {
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  Parse(#[from] serde_yaml::Error),
  #[error(transparent)]
  Client(#[from] NanoclClientError),
  #[error(transparent)]
  Docker(#[from] bollard::errors::Error),
  #[error(transparent)]
  Api(#[from] ApiError),
  #[error("{msg:?}")]
  Custom { msg: String },
}

impl CliError {
  pub fn exit(&self, args: &Cli) {
    match self {
      CliError::Client(err) => match err {
        nanocl_client::error::NanoclClientError::SendRequest(err) => {
          match err {
            ntex::http::client::error::SendRequestError::Connect(_) => {
              eprintln!(
              "Cannot connect to the nanocl daemon at {host}. Is the nanocl daemon running?",
              host = args.host
            )
            }
            _ => eprintln!("{err}"),
          }
        }
        nanocl_client::error::NanoclClientError::Api(err) => {
          eprintln!("Daemon [{}]: {}", err.status, err.msg);
        }
        _ => eprintln!("{err}"),
      },
      _ => eprintln!("{self}"),
    }
    std::process::exit(1);
  }
}
