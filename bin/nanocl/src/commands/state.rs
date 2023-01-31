use std::fs;

use nanocld_client::NanoclClient;

use crate::utils;
use crate::error::CliError;
use crate::models::{StateArgs, StateCommands, StateOpts};

async fn exec_apply(
  client: &NanoclClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&opts.file_path);
  let data = fs::read_to_string(file_path)?;
  // Todo check meta to send version and use correct api url
  let _meta = utils::state::get_file_meta(&data)?;

  let yaml: serde_yaml::Value = serde_yaml::from_str(data.as_str())?;

  let data = serde_json::to_value(yaml)?;

  client.apply_state(&data).await?;

  Ok(())
}

async fn exec_revert(
  client: &NanoclClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&opts.file_path);
  let data = fs::read_to_string(file_path)?;
  // Todo check meta to send version and use correct api url
  let _meta = utils::state::get_file_meta(&data)?;

  let yaml: serde_yaml::Value = serde_yaml::from_str(data.as_str())?;

  let data = serde_json::to_value(yaml)?;

  client.revert_state(&data).await?;

  Ok(())
}

pub async fn exec_state(
  client: &NanoclClient,
  args: &StateArgs,
) -> Result<(), CliError> {
  match &args.commands {
    StateCommands::Apply(opts) => exec_apply(client, opts).await,
    StateCommands::Revert(opts) => exec_revert(client, opts).await,
  }
}
