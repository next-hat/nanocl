use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::config::CliConfig;

pub async fn exec_info(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let info = client.info().await?;
  utils::print::print_yml(info)?;
  Ok(())
}
