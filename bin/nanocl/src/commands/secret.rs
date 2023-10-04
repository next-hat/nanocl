use nanocl_utils::io_error::{IoResult, FromIo};

use crate::utils;
use crate::config::CliConfig;
use crate::models::{
  SecretArg, SecretCommand, SecretRow, SecretRemoveOpts, SecretInspectOpts,
};

/// ## Exec secret list
///
/// Function that execute when running `nanocl secret ls`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_secret_ls(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let secrets = client.list_secret().await?;
  let rows = secrets
    .iter()
    .map(|s| SecretRow::from(s.clone()))
    .collect::<Vec<SecretRow>>();
  utils::print::print_table(rows);
  Ok(())
}

/// ## Exec secret rm
///
/// Function that execute when running `nanocl secret rm`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](SecretRemoveOpts) The secret remove options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_secret_rm(
  cli_conf: &CliConfig,
  opts: &SecretRemoveOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  if !opts.skip_confirm {
    utils::dialog::confirm(&format!("Delete secret {}?", opts.keys.join(",")))
      .map_err(|err| err.map_err_context(|| "Delete secret"))?;
  }
  for key in &opts.keys {
    client.delete_secret(key).await?;
  }
  Ok(())
}

/// ## Exec secret inspect
///
/// Function that execute when running `nanocl secret inspect`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](SecretInspectOpts) The secret inspect options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
async fn exec_secret_inspect(
  cli_conf: &CliConfig,
  opts: &SecretInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let secret = client.inspect_secret(&opts.key).await?;
  let _ = utils::print::display_format(
    &opts.display.clone().unwrap_or_default(),
    secret,
  );
  Ok(())
}

/// ## Exec secret
///
/// Function that execute when running `nanocl secret`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](SecretArg) The secret options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
pub async fn exec_secret(
  cli_conf: &CliConfig,
  args: &SecretArg,
) -> IoResult<()> {
  match &args.command {
    SecretCommand::List => exec_secret_ls(cli_conf).await,
    SecretCommand::Remove(opts) => exec_secret_rm(cli_conf, opts).await,
    SecretCommand::Inspect(opts) => exec_secret_inspect(cli_conf, opts).await,
  }
}
