use tabled::Tabled;
use clap::{Parser, Subcommand};
use nanocld_client::stubs::node::Node;

use super::GenericListOpts;

/// `nanocl node` available arguments
#[derive(Clone, Parser)]
#[clap(name = "nanocl-resource")]
pub struct NodeArg {
  #[clap(subcommand)]
  pub command: NodeCommand,
}

/// `nanocl node` available commands
#[derive(Clone, Subcommand)]
pub enum NodeCommand {
  /// List nodes
  #[clap(alias = "ls")]
  List(GenericListOpts),
}

/// A row of the node table
#[derive(Tabled)]
pub struct NodeRow {
  pub name: String,
  pub ip_address: String,
}

/// Convert a Node to a NodeRow
impl From<Node> for NodeRow {
  fn from(node: Node) -> Self {
    Self {
      name: node.name,
      ip_address: node.ip_address,
    }
  }
}
