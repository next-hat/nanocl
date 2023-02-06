use tabled::Tabled;
use clap::{Parser, Subcommand};
use nanocld_client::stubs::resource::{ResourceKind, Resource};

/// Resource commands
#[derive(Debug, Subcommand)]
pub enum ResourceCommands {
  /// Remove existing resource
  #[clap(alias("rm"))]
  Remove(ResourceRemoveOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List,
  /// Inspect a resource
  Inspect(ResourceInspectOpts),
  /// Browse history of a resource
  History(ResourceHistoryOpts),
  /// Reset a resource to a specific history
  Reset(ResourceResetOpts),
}

/// Manage resources
#[derive(Debug, Parser)]
#[clap(name = "nanocl-resource")]
pub struct ResourceArgs {
  #[clap(subcommand)]
  pub commands: ResourceCommands,
}

#[derive(Debug, Tabled)]
pub struct ResourceRow {
  pub name: String,
  pub kind: ResourceKind,
}

impl From<Resource> for ResourceRow {
  fn from(resource: Resource) -> Self {
    Self {
      name: resource.name,
      kind: resource.kind,
    }
  }
}

#[derive(Debug, Parser)]
pub struct ResourceRemoveOpts {
  /// The names of the resources to delete
  pub names: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct ResourceInspectOpts {
  /// The name of the resource to inspect
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct ResourceHistoryOpts {
  /// The name of the resource to browse history
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct ResourceResetOpts {
  /// The name of the resource to reset
  pub name: String,
  /// The key of the history to reset to
  pub key: String,
}
