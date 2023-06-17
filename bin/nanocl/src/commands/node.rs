use nanocld_client::NanocldClient;
use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::models::{NodeArgs, NodeCommands, NodeRow};

pub async fn exec_node(
  client: &NanocldClient,
  args: &NodeArgs,
) -> IoResult<()> {
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
