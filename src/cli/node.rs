use crate::client::Nanocld;
use crate::models::{NodeArgs, NodeCommands};

use super::errors::CliError;

pub async fn exec_node(
  client: &Nanocld,
  args: &NodeArgs,
) -> Result<(), CliError> {
  match &args.subcommands {
    NodeCommands::Create(node) => {
      todo!("create node {:#?}", node);
    }
  }
}
