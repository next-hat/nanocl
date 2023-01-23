use nanocl_client::NanoclClient;

use crate::error::CliError;
use crate::models::{ResourceArgs, ResourceCommands};

async fn exec_list(client: &NanoclClient) -> Result<(), CliError> {
  Ok(())
}

pub async fn exec_resource(
  client: &NanoclClient,
  args: &ResourceArgs,
) -> Result<(), CliError> {
  match args.commands {
    ResourceCommands::List => exec_list(client).await,
  }
}
