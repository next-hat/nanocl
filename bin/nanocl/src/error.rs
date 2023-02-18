use thiserror::Error;

use nanocld_client::error::{NanoclClientError, ApiError};

use crate::models::Cli;

#[derive(Debug, Error)]
pub enum CliError {
  #[error(transparent)]
  Io(#[from] std::io::Error),
  #[error(transparent)]
  ParseYml(#[from] serde_yaml::Error),
  #[error(transparent)]
  Client(#[from] NanoclClientError),
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
  pub fn exit(&self, args: &Cli) {
    match self {
      CliError::Client(err) => match err {
        nanocld_client::error::NanoclClientError::SendRequest(err) => match err
        {
          ntex::http::client::error::SendRequestError::Connect(_) => {
            eprintln!(
              "Cannot connect to the nanocl daemon at {host}. Is the nanocl daemon running?",
              host = args.host
            )
          }
          _ => eprintln!("{err}"),
        },
        nanocld_client::error::NanoclClientError::Api(err) => {
          eprintln!("Daemon [{}]: {}", err.status, err.msg);
        }
        _ => eprintln!("{err}"),
      },
      _ => eprintln!("{self}"),
    }
    std::process::exit(1);
  }
}
