use nanocl_utils::io_error::{IoResult, FromIo};

use crate::config::CliConfig;
use crate::utils;
use crate::models::{
  ResourceArgs, ResourceCommands, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceRevertOpts, ResourceHistoryOpts,
  ResourceListOpts,
};

async fn exec_resource_ls(
  cli_conf: &CliConfig,
  opts: &ResourceListOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resources = client.list_resource(None).await?;
  let row = resources
    .into_iter()
    .map(ResourceRow::from)
    .collect::<Vec<ResourceRow>>();
  match opts.quiet {
    true => {
      for row in row {
        println!("{}", row.name);
      }
    }
    false => {
      utils::print::print_table(row);
    }
  }
  Ok(())
}

async fn exec_resource_rm(
  cli_conf: &CliConfig,
  options: &ResourceRemoveOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  if !options.skip_confirm {
    utils::dialog::confirm(&format!(
      "Delete resource {}?",
      options.names.join(",")
    ))
    .map_err(|err| err.map_err_context(|| "Delete resource"))?;
  }
  for name in &options.names {
    client.delete_resource(name).await?;
  }
  Ok(())
}

async fn exec_resource_inspect(
  cli_conf: &CliConfig,
  opts: &ResourceInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resource = client.inspect_resource(&opts.name).await?;
  let display = opts
    .display
    .clone()
    .unwrap_or(cli_conf.user_config.display_format.clone());
  utils::print::display_format(&display, resource)?;
  Ok(())
}

async fn exec_resource_history(
  cli_conf: &CliConfig,
  opts: &ResourceHistoryOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let history = client.list_history_resource(&opts.name).await?;
  utils::print::print_yml(history)?;
  Ok(())
}

async fn exec_resource_revert(
  cli_conf: &CliConfig,
  opts: &ResourceRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resource = client.revert_resource(&opts.name, &opts.key).await?;
  utils::print::print_yml(resource)?;
  Ok(())
}

pub async fn exec_resource(
  cli_conf: &CliConfig,
  args: &ResourceArgs,
) -> IoResult<()> {
  match &args.commands {
    ResourceCommands::List(opts) => exec_resource_ls(cli_conf, opts).await,
    ResourceCommands::Remove(opts) => exec_resource_rm(cli_conf, opts).await,
    ResourceCommands::Inspect(opts) => {
      exec_resource_inspect(cli_conf, opts).await
    }
    ResourceCommands::History(opts) => {
      exec_resource_history(cli_conf, opts).await
    }
    ResourceCommands::Revert(opts) => {
      exec_resource_revert(cli_conf, opts).await
    }
  }
}
