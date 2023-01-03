use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocl_models::namespace::Namespace;

/// Namespace commands
#[derive(Debug, Subcommand)]
pub enum NamespaceCommands {
  /// Create new namespace
  Create(NamespaceOpts),
  /// Inspect a namespace
  Inspect(NamespaceOpts),
  /// Remove a namespace
  #[clap(alias("rm"))]
  Remove(NamespaceOpts),
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
pub struct NamespaceOpts {
  /// name of the namespace to create
  pub name: String,
}

#[derive(Tabled)]
pub struct NamespaceRow {
  pub(crate) name: String,
}

impl From<Namespace> for NamespaceRow {
  fn from(item: Namespace) -> Self {
    Self { name: item.name }
  }
}
