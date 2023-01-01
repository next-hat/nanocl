use tabled::Tabled;
use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

/// Namespace commands
#[derive(Debug, Subcommand)]
pub enum NamespaceCommands {
  /// Create new namespace
  Create(NamespacePartial),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List,
}

/// Manage namespaces
#[derive(Debug, Parser)]
#[clap(name = "nanocl-namespace")]
pub struct NamespaceArgs {
  #[clap(subcommand)]
  pub commands: NamespaceCommands,
}

#[derive(Debug, Parser)]
#[clap(name = "nanocl-namespace-create")]
pub struct NamespacePartial {
  /// name of the namespace to create
  pub name: String,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NamespaceItem {
  pub name: String,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NamespaceWithCount {
  pub(crate) name: String,
  pub(crate) cargoes: usize,
  pub(crate) clusters: usize,
  pub(crate) networks: usize,
}
