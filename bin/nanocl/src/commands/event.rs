use futures::StreamExt;

use nanocl_error::io::IoResult;
use nanocld_client::stubs::system::Event;

use crate::{
  utils,
  config::CliConfig,
  models::{EventArg, EventRow, EventCommand},
};

use super::{GenericCommand, GenericCommandInspect, GenericCommandLs};

impl GenericCommand for EventArg {
  fn object_name() -> &'static str {
    "events"
  }
}

impl GenericCommandLs for EventArg {
  type Item = EventRow;
  type Args = EventArg;
  type ApiItem = Event;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
}

impl GenericCommandInspect for EventArg {
  type ApiItem = Event;
}

/// Function that execute when running `nanocl events`
/// Will print the events emitted by the daemon
pub async fn watch_event(cli_conf: &CliConfig) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut stream = client.watch_events(None).await?;
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
    EventCommand::Inspect(opts) => {
      EventArg::exec_inspect(cli_conf, opts, None).await
    }
    EventCommand::Watch => watch_event(cli_conf).await,
  }
}
