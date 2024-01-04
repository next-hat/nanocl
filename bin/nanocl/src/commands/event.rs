use futures::StreamExt;

use nanocl_error::io::IoResult;
use nanocld_client::stubs::system::Event;

use crate::{
  utils,
  config::CliConfig,
  models::{EventArg, EventRow, EventCommand},
};

use super::GenericList;

impl GenericList for EventArg {
  type Item = EventRow;
  type Args = EventArg;
  type ApiItem = Event;

  fn object_name() -> &'static str {
    "events"
  }

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
}

/// Function that execute when running `nanocl events`
/// Will print the events emited by the daemon
pub async fn watch_event(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client.watch_events().await?;
  while let Some(event) = stream.next().await {
    let event = event?;
    utils::print::display_format(&cli_conf.user_config.display_format, event)?;
  }
  Ok(())
}

/// Function that execute when running `nanocl event`
pub async fn exec_event(cli_conf: &CliConfig, args: &EventArg) -> IoResult<()> {
  match &args.command {
    EventCommand::List(opts) => {
      EventArg::exec_ls(&cli_conf.client, args, opts).await
    }
    EventCommand::Watch => watch_event(cli_conf).await,
  }
}
