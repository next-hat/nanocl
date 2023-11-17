use nanocl_error::io::IoResult;
use nanocld_client::NanocldClient;

use crate::config::CliConfig;
use crate::models::{
  ProcessOpts, ProcessRow, SystemArg, SystemHttpArg, SystemHttpCommand,
  SystemCommand,
};
use crate::utils;
use crate::utils::print::print_table;

/// ## Exec process
///
/// Function that execute when running `nanocl ps`
/// Will print the list of existing instances of cargoes and virtual machines
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](ProcessOpts) The process options
///
pub async fn exec_process(
  cli_conf: &CliConfig,
  args: &ProcessOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let opts = args.clone().into();
  let items = client.process(Some(&opts)).await?;
  let rows = items
    .into_iter()
    .map(ProcessRow::from)
    .collect::<Vec<ProcessRow>>();
  print_table(rows);
  Ok(())
}

/// ## Exec http
///
/// Function that execute when running `nanocl system http`
/// Will print the list of http request
///
/// ## Arguments
///
/// * [client](NanocldClient) The nanocl daemon client
/// * [opts](SystemHttpArg) The system http options
///
pub async fn exec_http(
  client: &NanocldClient,
  opts: &SystemHttpArg,
) -> IoResult<()> {
  match &opts.command {
    SystemHttpCommand::Logs(opts) => {
      let logs = client.list_http_metric(Some(&opts.clone().into())).await?;
      utils::print::print_yml(logs)?;
    }
  }
  Ok(())
}

/// ## Exec system
///
/// Function that execute when running `nanocl system`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](SystemArg) The system options
///
pub async fn exec_system(
  cli_conf: &CliConfig,
  args: &SystemArg,
) -> IoResult<()> {
  let client = &cli_conf.client;
  match &args.command {
    SystemCommand::Http(opts) => exec_http(client, opts).await,
  }
}
