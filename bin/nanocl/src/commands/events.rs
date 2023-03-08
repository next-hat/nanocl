use futures::StreamExt;

use nanocld_client::NanocldClient;

use crate::{error::CliError, utils::print::print_yml};

pub async fn exec_events(client: &NanocldClient) -> Result<(), CliError> {
  let mut stream = client.watch_events().await?;

  while let Some(event) = stream.next().await {
    let event = event?;
    let _ = print_yml(event);
  }

  Ok(())
}
