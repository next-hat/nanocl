use clap::{Parser, Subcommand};
use nanocld_client::stubs::node::Node;
use tabled::Tabled;

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
  /// Endpoint of the node
  pub endpoint: String,
  /// Version of the node
  pub version: String,
  #[tabled(rename = "CREATED AT")]
  created_at: String,
  #[tabled(skip)]
  pub age: String,
}

/// A compact row of the node table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NodeCompactRow {
  pub name: String,
  pub version: String,
  pub age: String,
}

impl From<NodeRow> for NodeCompactRow {
  fn from(row: NodeRow) -> Self {
    Self {
      name: row.name,
      version: row.version,
      age: row.age,
    }
  }
}

/// Convert a Node to a NodeRow
impl From<Node> for NodeRow {
  fn from(node: Node) -> Self {
    let age =
      super::format_age(Some(&node.created_at.and_utc()), chrono::Utc::now());
    let created_at = node.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
    Self {
      name: node.name,
      endpoint: node.endpoint,
      version: node.version,
      created_at,
      age,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::GenericCommandLs;

  #[test]
  fn node_list_compact_wide_and_quiet() {
    let created_at =
      (chrono::Utc::now() - chrono::Duration::days(3)).naive_utc();
    let fixture = || Node {
      name: "worker-long-name".to_owned(),
      created_at,
      endpoint: "https://worker.example:8585".to_owned(),
      version: "0.19.0".to_owned(),
      metadata: None,
    };
    let row = NodeRow::from(fixture());
    assert_eq!(row.age, "3d");
    let exact_created_at = row.created_at.clone();
    let mut opts =
      GenericListOpts::<super::super::GenericDefaultOpts>::default();
    let compact = NodeArg::render_list(&opts, vec![row]);
    assert_eq!(NodeCompactRow::headers(), ["NAME", "VERSION", "AGE"]);
    assert!(compact.contains("worker-long-name"));
    assert!(compact.contains("0.19.0"));
    assert!(compact.contains("3d"));
    assert!(!compact.contains("ENDPOINT"));
    assert!(!compact.contains("https://worker.example:8585"));
    assert!(!compact.contains("CREATED AT"));

    opts.wide = true;
    let wide = NodeArg::render_list(&opts, vec![NodeRow::from(fixture())]);
    assert!(wide.contains("ENDPOINT"));
    assert!(wide.contains("https://worker.example:8585"));
    assert!(wide.contains("CREATED AT"));
    assert!(wide.contains(&exact_created_at));

    opts.quiet = true;
    assert_eq!(
      NodeArg::render_list(&opts, vec![NodeRow::from(fixture())]),
      "worker-long-name"
    );
  }
}
