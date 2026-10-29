use nanocl_error::io::IoResult;
use nanocld_client::stubs::resource::Resource;

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, GenericHistoryOpts, ResourceArg, ResourceCommand,
    ResourceCompactRow, ResourceRevertOpts, ResourceRow, SpecHistoryRow,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
};

impl GenericCommand for ResourceArg {
  fn object_name() -> &'static str {
    "resources"
  }
}

impl GenericCommandLs for ResourceArg {
  type Item = ResourceRow;
  type CompactItem = ResourceCompactRow;
  type Args = ResourceArg;
  type ApiItem = nanocld_client::stubs::resource::Resource;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for ResourceArg {}

impl GenericCommandInspect for ResourceArg {
  type ApiItem = Resource;
}

/// Function that execute when running `nanocl resource history`
async fn exec_resource_history(
  cli_conf: &CliConfig,
  opts: &GenericHistoryOpts,
) -> IoResult<()> {
  super::history::validate_options(opts)?;
  let history = cli_conf.client.list_history_resource(&opts.key).await?;
  super::history::print_revision_history(
    opts,
    &cli_conf.user_config.display_format,
    history,
    |item| item.key,
    SpecHistoryRow::from,
  )
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
      ResourceArg::exec_ls(&cli_conf.client, args, opts, None).await
    }
    ResourceCommand::Remove(opts) => {
      ResourceArg::exec_rm(&cli_conf.client, opts).await
    }
    ResourceCommand::Inspect(opts) => {
      ResourceArg::exec_inspect(cli_conf, opts).await
    }
    ResourceCommand::History(opts) => {
      exec_resource_history(cli_conf, opts).await
    }
    ResourceCommand::Revert(opts) => exec_resource_revert(cli_conf, opts).await,
  }
}
