use nanocl_utils::io_error::{IoResult, FromIo};

use crate::utils;
use crate::config::CliConfig;
use crate::models::{
  ResourceArg, ResourceCommand, ResourceRow, ResourceRemoveOpts,
  ResourceInspectOpts, ResourceRevertOpts, ResourceHistoryOpts,
  ResourceListOpts,
};

/// ## Exec resource ls
///
/// Function that execute when running `nanocl resource ls`
/// Will list available resources
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](ResourceListOpts) The resource list options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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

/// ## Exec resource rm
///
/// Function that execute when running `nanocl resource rm`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [options](ResourceRemoveOpts) The resource remove options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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

/// ## Exec resource inspect
///
/// Function that execute when running `nanocl resource inspect`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](ResourceInspectOpts) The resource inspect options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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

/// ## Exec resource history
///
/// Function that execute when running `nanocl resource history`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](ResourceHistoryOpts) The resource history options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_resource_history(
  cli_conf: &CliConfig,
  opts: &ResourceHistoryOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let history = client.list_history_resource(&opts.name).await?;
  utils::print::print_yml(history)?;
  Ok(())
}

/// ## Exec resource revert
///
/// Function that execute when running `nanocl resource revert`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](ResourceRevertOpts) The resource revert options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_resource_revert(
  cli_conf: &CliConfig,
  opts: &ResourceRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resource = client.revert_resource(&opts.name, &opts.key).await?;
  utils::print::print_yml(resource)?;
  Ok(())
}

/// ## Exec resource
///
/// Function that execute when running `nanocl resource`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](ResourceArg) The resource options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
pub async fn exec_resource(
  cli_conf: &CliConfig,
  args: &ResourceArg,
) -> IoResult<()> {
  match &args.command {
    ResourceCommand::List(opts) => exec_resource_ls(cli_conf, opts).await,
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
