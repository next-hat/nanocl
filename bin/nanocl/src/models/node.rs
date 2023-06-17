use nanocld_client::stubs::node::Node;
use tabled::Tabled;
use clap::{Parser, Subcommand};

/// Manage resources
#[derive(Debug, Parser)]
#[clap(name = "nanocl-resource")]
pub struct NodeArgs {
  #[clap(subcommand)]
  pub commands: NodeCommands,
}

#[derive(Debug, Subcommand)]
pub enum NodeCommands {
  /// List nodes
  #[clap(alias = "ls")]
  List,
}

#[derive(Debug, Tabled)]
pub struct NodeRow {
  pub name: String,
  pub ip_address: String,
}

impl From<Node> for NodeRow {
  fn from(node: Node) -> Self {
    Self {
      name: node.name,
      ip_address: node.ip_address,
    }
  }
}
