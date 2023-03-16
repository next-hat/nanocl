use nanocld_client::NanocldClient;

use crate::models::{ProcessOpts, ProcessRow};
use crate::error::CliError;
use crate::utils::print::print_table;

pub async fn exec_process(
  client: &NanocldClient,
  options: &ProcessOpts,
) -> Result<(), CliError> {
  let opts = options.clone().into();

  let items = client.process(Some(opts)).await?;

  let rows = items
    .into_iter()
    .map(ProcessRow::from)
    .collect::<Vec<ProcessRow>>();
  print_table(rows);

  Ok(())
}
