use tabled::Tabled;
use clap::{Parser, Subcommand};
use nanocld_client::stubs::node::Node;

use super::GenericListOpts;

/// `nanocl node` available arguments
#[derive(Clone, Parser)]
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
#[tabled(rename_all = "UPPERCASE")]
pub struct NodeRow {
  /// Name of the node
  pub name: String,
  /// IP address of the node
  pub ip_address: String,
  /// Endpoint of the node
  pub endpoint: String,
  /// Version of the node
  pub version: String,
  #[tabled(rename = "CREATED AT")]
  created_at: String,
}

/// Convert a Node to a NodeRow
impl From<Node> for NodeRow {
  fn from(node: Node) -> Self {
    let created_at = node.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
    Self {
      name: node.name,
      ip_address: node.ip_address.to_string(),
      endpoint: node.endpoint,
      version: node.version,
      created_at,
    }
  }
}
