use nanocld_client::NanocldClient;

use crate::utils::print::*;
use crate::error::CliError;
use crate::models::{
  ResourceArgs, ResourceCommands, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceResetOpts, ResourceHistoryOpts,
};

// Since Resource are random json config
// we can't really validate them using the cli
// so we cannot create them using a create command
// but we can create them using a apply command
// which will apply a state file
//
// async fn exec_create
//   client: &NanocldClient,
//   opts: &ResourceCreateOpts,
// ) -> Result<(), CliError> {
//   let mut file_path = std::env::current_dir()?;
//   file_path.push(&opts.file_path);
//   let data = fs::read_to_string(file_path)?;

//   let meta = utils::state::get_file_meta(&data)?;

//   if meta.r#type != "Resource" {
//     return Err(CliError::Custom {
//       msg: format!("Invalid file type expected resource got: {}", &meta.r#type),
//     });
//   }

//   let resources = utils::state::get_resources(&data)?;

//   for resource in resources.resources {
//     client.create_resource(&resource).await?;
//   }

//   Ok(())
// }

async fn exec_list(client: &NanocldClient) -> Result<(), CliError> {
  let resources = client.list_resource(None).await?;

  let row = resources
    .into_iter()
    .map(ResourceRow::from)
    .collect::<Vec<ResourceRow>>();

  print_table(row);
  Ok(())
}

async fn exec_remove(
  client: &NanocldClient,
  opts: &ResourceRemoveOpts,
) -> Result<(), CliError> {
  for name in &opts.names {
    client.delete_resource(name).await?;
  }

  Ok(())
}

async fn exec_inspect(
  client: &NanocldClient,
  opts: &ResourceInspectOpts,
) -> Result<(), CliError> {
  let resource = client.inspect_resource(&opts.name).await?;

  print_yml(resource)?;
  Ok(())
}

async fn exec_history(
  client: &NanocldClient,
  opts: &ResourceHistoryOpts,
) -> Result<(), CliError> {
  let history = client.list_history_resource(&opts.name).await?;

  print_yml(history)?;
  Ok(())
}

async fn exec_reset(
  client: &NanocldClient,
  opts: &ResourceResetOpts,
) -> Result<(), CliError> {
  let resource = client.reset_resource(&opts.name, &opts.key).await?;

  print_yml(resource)?;
  Ok(())
}

pub async fn exec_resource(
  client: &NanocldClient,
  args: &ResourceArgs,
) -> Result<(), CliError> {
  match &args.commands {
    // ResourceCommands::Create(opts) => exec_create(client, opts).await,
    ResourceCommands::List => exec_list(client).await,
    ResourceCommands::Remove(opts) => exec_remove(client, opts).await,
    ResourceCommands::Inspect(opts) => exec_inspect(client, opts).await,
    ResourceCommands::History(opts) => exec_history(client, opts).await,
    ResourceCommands::Reset(opts) => exec_reset(client, opts).await,
  }
}
