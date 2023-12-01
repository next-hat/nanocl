use nanocl_error::io::IoResult;

use crate::config::CliConfig;
use crate::models::{ProcessOpts, ProcessRow};
use crate::utils::print::print_table;

/// Function that execute when running `nanocl ps`
/// Will print the list of existing instances of cargoes and virtual machines
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
