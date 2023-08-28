use futures::StreamExt;

use nanocl_utils::io_error::IoResult;

use crate::{utils::print::print_yml, config::CliConfig};

pub async fn exec_events(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client.watch_events().await?;

  while let Some(event) = stream.next().await {
    let event = event?;
    print_yml(event)?;
  }

  Ok(())
}
