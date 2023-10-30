use futures::StreamExt;

use nanocl_error::io::IoResult;

use crate::{utils::print::print_yml, config::CliConfig};

/// ## Exec events
///
/// Function that execute when running `nanocl events`
/// Will print the events emited by the daemon
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) An error occured
///
pub async fn exec_events(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client.watch_events().await?;
  while let Some(event) = stream.next().await {
    let event = event?;
    print_yml(event)?;
  }
  Ok(())
}
