use nanocl_utils::io_error::IoResult;
use nanocld_client::NanocldClient;

use crate::models::{
  ProcessOpts, ProcessRow, SystemOpts, SystemHttpOpts, SystemHttpCommands,
  SystemCommands,
};
use crate::utils;
use crate::utils::print::print_table;

pub async fn exec_process(
  client: &NanocldClient,
  options: &ProcessOpts,
) -> IoResult<()> {
  let opts = options.clone().into();

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
  client: &NanocldClient,
  opts: &SystemOpts,
) -> IoResult<()> {
  match &opts.commands {
    SystemCommands::Http(opts) => exec_http(client, opts).await,
  }
}
