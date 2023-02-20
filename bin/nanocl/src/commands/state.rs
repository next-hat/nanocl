use std::fs;

use nanocld_client::NanocldClient;

use crate::utils;
use crate::error::CliError;
use crate::models::{StateArgs, StateCommands, StateOpts};

async fn exec_apply(
  client: &NanocldClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  match url::Url::parse(&opts.file_path) {
    Ok(url) => {
      let reqwest = ntex::http::Client::default();
      let data = reqwest
        .get(url.to_string())
        .send()
        .await
        .map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from url: {err}"),
        })?
        .body()
        .await
        .map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from url: {err}"),
        })?;
      let data = data.to_vec();
      let data =
        std::str::from_utf8(&data).map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from url: {err}"),
        })?;
      let _meta = utils::state::get_file_meta(&String::from(data))?;
      let yaml: serde_yaml::Value = serde_yaml::from_str(data)?;
      let data = serde_json::to_value(yaml)?;
      client.apply_state(&data).await?;
      return Ok(());
    }
    Err(_) => {
      let mut file_path = std::env::current_dir()?;
      file_path.push(&opts.file_path);
      let data = fs::read_to_string(file_path)?;
      // Todo check meta to send version and use correct api url
      let _meta = utils::state::get_file_meta(&data)?;

      let yaml: serde_yaml::Value = serde_yaml::from_str(data.as_str())?;

      let data = serde_json::to_value(yaml)?;

      client.apply_state(&data).await?;
    }
  }

  Ok(())
}

async fn exec_revert(
  client: &NanocldClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  let data = match url::Url::parse(&opts.file_path) {
    Ok(url) => {
      let reqwest = ntex::http::Client::default();
      let data = reqwest
        .get(url.to_string())
        .send()
        .await
        .map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from {url}: {err}"),
        })?
        .body()
        .await
        .map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from {url}: {err}"),
        })?;
      std::str::from_utf8(&data)
        .map_err(|err| CliError::Custom {
          msg: format!("Cannot fetch state file from url: {err}"),
        })?
        .to_string()
    }
    Err(_) => {
      let mut file_path = std::env::current_dir()?;
      file_path.push(&opts.file_path);
      fs::read_to_string(file_path)?
    }
  };
  let _meta = utils::state::get_file_meta(&String::from(&data))?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(&data)?;
  let data = serde_json::to_value(yaml)?;
  client.revert_state(&data).await?;
  Ok(())
}

pub async fn exec_state(
  client: &NanocldClient,
  args: &StateArgs,
) -> Result<(), CliError> {
  match &args.commands {
    StateCommands::Apply(opts) => exec_apply(client, opts).await,
    StateCommands::Revert(opts) => exec_revert(client, opts).await,
  }
}
