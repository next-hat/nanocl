use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;

use nanocl_utils::io_error::{IoError, IoResult};
use nanocld_client::NanocldClient;

use crate::utils::print::{print_yml, print_table};
use crate::models::{
  ResourceArgs, ResourceCommands, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceRevertOpts, ResourceHistoryOpts,
};

async fn exec_resource_ls(client: &NanocldClient) -> IoResult<()> {
  let resources = client.list_resource(None).await?;

  let row = resources
    .into_iter()
    .map(ResourceRow::from)
    .collect::<Vec<ResourceRow>>();

  print_table(row);
  Ok(())
}

async fn exec_resource_rm(
  client: &NanocldClient,
  options: &ResourceRemoveOpts,
) -> IoResult<()> {
  if !options.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt(format!("Delete resources {}?", options.names.join(",")))
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(IoError::interupted(
          "Resource remove",
          "interupted by user",
        ))
      }
    }
  }
  for name in &options.names {
    client.delete_resource(name).await?;
  }

  Ok(())
}

async fn exec_resource_inspect(
  client: &NanocldClient,
  opts: &ResourceInspectOpts,
) -> IoResult<()> {
  let resource = client.inspect_resource(&opts.name).await?;

  print_yml(resource)?;
  Ok(())
}

async fn exec_resource_history(
  client: &NanocldClient,
  opts: &ResourceHistoryOpts,
) -> IoResult<()> {
  let history = client.list_history_resource(&opts.name).await?;

  print_yml(history)?;
  Ok(())
}

async fn exec_resource_revert(
  client: &NanocldClient,
  opts: &ResourceRevertOpts,
) -> IoResult<()> {
  let resource = client.revert_resource(&opts.name, &opts.key).await?;

  print_yml(resource)?;
  Ok(())
}

pub async fn exec_resource(
  client: &NanocldClient,
  args: &ResourceArgs,
) -> IoResult<()> {
  match &args.commands {
    // ResourceCommands::Create(opts) => exec_create(client, opts).await,
    ResourceCommands::List => exec_resource_ls(client).await,
    ResourceCommands::Remove(opts) => exec_resource_rm(client, opts).await,
    ResourceCommands::Inspect(opts) => {
      exec_resource_inspect(client, opts).await
    }
    ResourceCommands::History(opts) => {
      exec_resource_history(client, opts).await
    }
    ResourceCommands::Revert(opts) => exec_resource_revert(client, opts).await,
  }
}
