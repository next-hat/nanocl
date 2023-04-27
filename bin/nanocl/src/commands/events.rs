use futures::StreamExt;

use nanocl_utils::io_error::IoResult;
use nanocld_client::NanocldClient;

use crate::utils::print::print_yml;

pub async fn exec_events(client: &NanocldClient) -> IoResult<()> {
  let mut stream = client.watch_events().await?;

  while let Some(event) = stream.next().await {
    let event = event?;
    print_yml(event)?;
  }

  Ok(())
}
