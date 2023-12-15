use nanocl_error::io::{IoResult, FromIo};

use crate::utils;
use crate::config::CliConfig;
use crate::models::{
  ResourceArg, ResourceCommand, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceRevertOpts, ResourceHistoryOpts,
};

use super::GenericList;

impl GenericList for ResourceArg {
  type Item = ResourceRow;
  type Args = ResourceArg;
  type ApiItem = nanocld_client::stubs::resource::Resource;

  fn object_name() -> &'static str {
    "resources"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

/// Function that execute when running `nanocl resource rm`
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

/// Function that execute when running `nanocl resource inspect`
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

/// Function that execute when running `nanocl resource history`
async fn exec_resource_history(
  cli_conf: &CliConfig,
  opts: &ResourceHistoryOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let history = client.list_history_resource(&opts.name).await?;
  utils::print::print_yml(history)?;
  Ok(())
}

/// Function that execute when running `nanocl resource revert`
async fn exec_resource_revert(
  cli_conf: &CliConfig,
  opts: &ResourceRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resource = client.revert_resource(&opts.name, &opts.key).await?;
  utils::print::print_yml(resource)?;
  Ok(())
}

/// Function that execute when running `nanocl resource`
pub async fn exec_resource(
  cli_conf: &CliConfig,
  args: &ResourceArg,
) -> IoResult<()> {
  match &args.command {
    ResourceCommand::List(opts) => {
      ResourceArg::exec_ls(&cli_conf.client, args, opts).await??;
      Ok(())
    }
    ResourceCommand::Remove(opts) => exec_resource_rm(cli_conf, opts).await,
    ResourceCommand::Inspect(opts) => {
      exec_resource_inspect(cli_conf, opts).await
    }
    ResourceCommand::History(opts) => {
      exec_resource_history(cli_conf, opts).await
    }
    ResourceCommand::Revert(opts) => exec_resource_revert(cli_conf, opts).await,
  }
}
