use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::config::CliConfig;
use crate::models::{NodeArg, NodeCommand, NodeRow};

/// ## Exec node
///
/// Function that execute when running `nanocl node`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](NodeArg) The node options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_utils::io_error::IoError) An error occured
///
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
