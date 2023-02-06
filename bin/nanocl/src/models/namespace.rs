use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::namespace::NamespaceSummary;

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
  pub(crate) cargoes: i64,
  pub(crate) instances: i64,
}

impl From<NamespaceSummary> for NamespaceRow {
  fn from(item: NamespaceSummary) -> Self {
    Self {
      name: item.name,
      cargoes: item.cargoes,
      instances: item.instances,
    }
  }
}
