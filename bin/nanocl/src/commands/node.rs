use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::config::CommandConfig;
use crate::models::{NodeArgs, NodeCommands, NodeRow};

pub async fn exec_node(cmd_conf: &CommandConfig<&NodeArgs>) -> IoResult<()> {
  let args = cmd_conf.args;
  let client = &cmd_conf.client;
  match args.commands {
    NodeCommands::List => {
      let nodes = client
        .list_node()
        .await?
        .into_iter()
        .map(NodeRow::from)
        .collect::<Vec<_>>();
      utils::print::print_table(nodes);
    }
  }
  Ok(())
}
