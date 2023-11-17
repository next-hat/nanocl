use nanocl_error::io::IoResult;

use crate::utils;
use crate::config::CliConfig;

/// ## Exec info
///
/// Function that execute when running `nanocl info`
/// Will print the info of the daemon
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
///
pub async fn exec_info(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let info = client.info().await?;
  utils::print::print_yml(info)?;
  Ok(())
}
