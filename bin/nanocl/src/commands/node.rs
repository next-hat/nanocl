use nanocl_error::io::IoResult;

use crate::utils;
use crate::config::CliConfig;
use crate::models::{NodeArg, NodeCommand, NodeRow};

/// Function that execute when running `nanocl node`
pub async fn exec_node(cli_conf: &CliConfig, args: &NodeArg) -> IoResult<()> {
  let client = &cli_conf.client;
  match args.command {
    NodeCommand::List => {
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
