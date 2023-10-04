use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::namespace::NamespaceSummary;

/// ## NamespaceCommand
///
/// `nanocl namespace` available commands
///
#[derive(Debug, Subcommand)]
pub enum NamespaceCommand {
  /// Create new namespace
  Create(NamespaceOpts),
  /// Inspect a namespace
  Inspect(NamespaceOpts),
  /// Remove a namespace
  #[clap(alias("rm"))]
  Remove(NamespaceDeleteOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List(NamespaceListOpts),
}

/// ## NamespaceListOpts
///
/// `nanocl namespace list` available options
///
#[derive(Debug, Parser)]
pub struct NamespaceListOpts {
  /// Show only namespace names
  #[clap(long, short)]
  pub quiet: bool,
}

/// ## NamespaceDeleteOpts
///
/// `nanocl namespace delete` available options
///
#[derive(Debug, Parser)]
pub struct NamespaceDeleteOpts {
  /// skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// list of namespace names to delete
  pub names: Vec<String>,
}

/// ## NamespaceArg
///
/// `nanocl namespace` available arguments
///
#[derive(Debug, Parser)]
#[clap(name = "nanocl namespace")]
pub struct NamespaceArg {
  #[clap(subcommand)]
  pub command: NamespaceCommand,
}

/// ## NamespaceOpts
///
/// `nanocl namespace create` and `nanocl namespace inspect` generic name option
///
#[derive(Debug, Parser)]
pub struct NamespaceOpts {
  /// name of the namespace to create
  pub name: String,
}

/// ## NamespaceRow
///
/// A row of the namespace table
///
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NamespaceRow {
  /// Name of the namespace
  pub(crate) name: String,
  /// Number of cargoes
  pub(crate) cargoes: i64,
  /// Number of instances
  pub(crate) instances: i64,
  /// Default gateway of the namespace
  pub(crate) gateway: String,
}

/// Convert a NamespaceSummary to a NamespaceRow
impl From<NamespaceSummary> for NamespaceRow {
  fn from(item: NamespaceSummary) -> Self {
    Self {
      name: item.name,
      cargoes: item.cargoes,
      instances: item.instances,
      gateway: item.gateway,
    }
  }
}
