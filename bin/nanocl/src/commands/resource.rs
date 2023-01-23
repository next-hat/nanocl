use std::fs;

use nanocl_client::NanoclClient;

use crate::utils;
use crate::error::CliError;
use crate::models::{
  ResourceArgs, ResourceCommands, ResourceRow, ResourceCreateOpts,
  ResourceRemoveOpts, ResourceInspectOpts,
};

use super::utils::print_table;

async fn exec_create(
  client: &NanoclClient,
  opts: &ResourceCreateOpts,
) -> Result<(), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(&opts.file_path);
  let data = fs::read_to_string(file_path)?;

  let meta = utils::yml::get_file_meta(&data)?;

  if meta.r#type != "resource" {
    return Err(CliError::Custom {
      msg: format!("Invalid file type expected resource got: {}", &meta.r#type),
    });
  }

  let resources = utils::yml::get_resources(&data)?;

  for resource in resources.resources {
    client.create_resource(&resource).await?;
  }

  Ok(())
}

async fn exec_list(client: &NanoclClient) -> Result<(), CliError> {
  let resources = client.list_resource().await?;

  let row = resources
    .into_iter()
    .map(ResourceRow::from)
    .collect::<Vec<ResourceRow>>();

  print_table(row);
  Ok(())
}

async fn exec_remove(
  client: &NanoclClient,
  opts: &ResourceRemoveOpts,
) -> Result<(), CliError> {
  for name in &opts.names {
    client.delete_resource(name).await?;
  }

  Ok(())
}

async fn exec_inspect(
  client: &NanoclClient,
  opts: &ResourceInspectOpts,
) -> Result<(), CliError> {
  let resource = client.inspect_resource(&opts.name).await?;

  let resource = serde_yaml::to_string(&resource)?;
  println!("{}", &resource);

  Ok(())
}

pub async fn exec_resource(
  client: &NanoclClient,
  args: &ResourceArgs,
) -> Result<(), CliError> {
  match &args.commands {
    ResourceCommands::Create(opts) => exec_create(client, opts).await,
    ResourceCommands::List => exec_list(client).await,
    ResourceCommands::Remove(opts) => exec_remove(client, opts).await,
    ResourceCommands::Inspect(opts) => exec_inspect(client, opts).await,
  }
}
