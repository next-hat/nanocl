use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use nanocld_client::NanocldClient;

use crate::utils::print::*;
use crate::error::CliError;
use crate::models::{
  ResourceArgs, ResourceCommands, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceResetOpts, ResourceHistoryOpts,
};

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
  options: &ResourceRemoveOpts,
) -> Result<(), CliError> {
  if !options.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt(format!("Delete resources {}?", options.names.join(",")))
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(CliError::Custom {
          msg: "Aborted".into(),
        })
      }
    }
  }
  for name in &options.names {
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
