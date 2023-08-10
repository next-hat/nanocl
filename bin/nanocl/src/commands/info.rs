use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::config::CommandConfig;

pub async fn exec_info(cmd_conf: &CommandConfig<Option<u8>>) -> IoResult<()> {
  let client = &cmd_conf.client;
  let info = client.info().await?;
  utils::print::print_yml(info)?;
  Ok(())
}
