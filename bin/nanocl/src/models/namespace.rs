use chrono::TimeZone;
use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::namespace::NamespaceSummary;

use super::{GenericInspectOpts, GenericListOpts, GenericRemoveOpts};

/// `nanocl namespace` available commands
#[derive(Clone, Subcommand)]
pub enum NamespaceCommand {
  /// Create new namespace
  Create(NamespaceCreateOpts),
  /// Inspect a namespace
  Inspect(GenericInspectOpts),
  /// Remove a namespace
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List(GenericListOpts),
}

/// `nanocl namespace delete` available options
#[derive(Clone, Parser)]
pub struct NamespaceDeleteOpts {
  /// skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// list of namespace names to delete
  pub names: Vec<String>,
}

/// `nanocl namespace` available arguments
#[derive(Clone, Parser)]
#[clap(name = "nanocl namespace")]
pub struct NamespaceArg {
  #[clap(subcommand)]
  pub command: NamespaceCommand,
}

/// `nanocl namespace create` and `nanocl namespace inspect` generic name option
#[derive(Clone, Parser)]
pub struct NamespaceCreateOpts {
  /// name of the namespace to create
  pub name: String,
}

/// A row of the namespace table
#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NamespaceRow {
  /// Name of the namespace
  pub name: String,
  /// Number of cargoes
  pub cargoes: usize,
  /// Number of instances
  pub instances: usize,
  /// Default gateway of the namespace
  pub gateway: String,
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
}

/// Convert a NamespaceSummary to a NamespaceRow
impl From<NamespaceSummary> for NamespaceRow {
  fn from(item: NamespaceSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(item.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: item.name,
      cargoes: item.cargoes,
      instances: item.instances,
      gateway: item.gateway,
      created_at: created_at.to_string(),
    }
  }
}
