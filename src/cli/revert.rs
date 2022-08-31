use std::path::PathBuf;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use ntex::http::StatusCode;

use crate::client::Nanocld;
use crate::client::error::NanocldError;
use crate::models::{RevertArgs, YmlConfigTypes, YmlNamespaceConfig};

use super::utils::get_config_type;
use super::errors::CliError;

async fn revert_namespace(
  namespace: &YmlNamespaceConfig,
  client: &Nanocld,
) -> Result<(), CliError> {
  // Delete cargoes
  namespace
    .cargoes
    .iter()
    .map(|cargo| async {
      let result = client
        .delete_cargo(&cargo.name, Some(namespace.name.to_owned()))
        .await;
      if let Err(err) = result {
        match err {
          NanocldError::Api(ref api_err) => {
            if api_err.status == StatusCode::NOT_FOUND {
              return Ok::<(), CliError>(());
            }
            return Err::<(), CliError>(CliError::Client(err));
          }
          _ => {
            return Err::<(), CliError>(CliError::Client(err));
          }
        }
      }
      Ok::<(), CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;

  // Delete clusters
  namespace
    .clusters
    .iter()
    .map(|cluster| async {
      let result = client
        .delete_cluster(&cluster.name, Some(namespace.name.to_owned()))
        .await;
      if let Err(err) = result {
        match err {
          NanocldError::Api(ref api_err) => {
            if api_err.status == StatusCode::NOT_FOUND {
              return Ok::<(), CliError>(());
            }
            return Err::<(), CliError>(CliError::Client(err));
          }
          _ => {
            return Err::<(), CliError>(CliError::Client(err));
          }
        }
      }
      Ok::<(), CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;
  Ok(())
}

async fn revert(file_path: PathBuf, client: &Nanocld) -> Result<(), CliError> {
  let file_content = std::fs::read_to_string(file_path)?;
  let config_type = get_config_type(&file_content)?;
  match config_type {
    YmlConfigTypes::Namespace => {
      let namespace =
        serde_yaml::from_str::<YmlNamespaceConfig>(&file_content)?;
      revert_namespace(&namespace, client).await?;
    }
    _ => todo!("delete different type of config"),
  }
  Ok(())
}

pub async fn exec_revert(
  client: &Nanocld,
  args: &RevertArgs,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&args.file_path);
  revert(file_path, client).await?;
  Ok(())
}
