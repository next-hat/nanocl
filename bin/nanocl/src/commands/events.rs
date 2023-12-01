use futures::StreamExt;

use nanocl_error::io::IoResult;

use crate::{utils::print::print_yml, config::CliConfig};

/// Function that execute when running `nanocl events`
/// Will print the events emited by the daemon
pub async fn exec_events(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client.watch_events().await?;
  while let Some(event) = stream.next().await {
    let event = event?;
    print_yml(event)?;
  }
  Ok(())
}
