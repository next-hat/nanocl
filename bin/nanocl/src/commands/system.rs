use nanocl_utils::io_error::IoResult;
use nanocld_client::NanocldClient;

use crate::config::CommandConfig;
use crate::models::{
  ProcessOpts, ProcessRow, SystemOpts, SystemHttpOpts, SystemHttpCommands,
  SystemCommands,
};
use crate::utils;
use crate::utils::print::print_table;

pub async fn exec_process(
  cmd_conf: &CommandConfig<&ProcessOpts>,
) -> IoResult<()> {
  let args = cmd_conf.args;
  let client = &cmd_conf.client;
  let opts = args.clone().into();
  let items = client.process(Some(opts)).await?;
  let rows = items
    .into_iter()
    .map(ProcessRow::from)
    .collect::<Vec<ProcessRow>>();
  print_table(rows);
  Ok(())
}

pub async fn exec_http(
  client: &NanocldClient,
  opts: &SystemHttpOpts,
) -> IoResult<()> {
  match &opts.commands {
    SystemHttpCommands::Logs(opts) => {
      let logs = client.list_http_metric(Some(opts.clone().into())).await?;
      utils::print::print_yml(logs)?;
    }
  }

  Ok(())
}

pub async fn exec_system(
  cmd_conf: &CommandConfig<&SystemOpts>,
) -> IoResult<()> {
  let args = cmd_conf.args;
  let client = &cmd_conf.client;
  match &args.commands {
    SystemCommands::Http(opts) => exec_http(client, opts).await,
  }
}
