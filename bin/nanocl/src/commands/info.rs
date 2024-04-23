use nanocl_error::io::IoResult;

use crate::{utils, config::CliConfig};

/// Function that execute when running `nanocl info`
/// Will print the info of the daemon
pub async fn exec_info(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let info = client.info().await?;
  utils::print::print_yml(info)?;
  Ok(())
}
