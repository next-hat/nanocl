use nanocl_utils::io_error::IoResult;
use nanocld_client::NanocldClient;

use crate::utils::print::print_yml;

pub async fn exec_info(client: &NanocldClient) -> IoResult<()> {
  let info = client.info().await?;

  print_yml(info)?;
  Ok(())
}
