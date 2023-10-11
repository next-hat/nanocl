use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::resource::Resource;

use super::DisplayFormat;

/// ## ResourceCommand
///
/// `nanocl resource` available commands
///
#[derive(Debug, Subcommand)]
pub enum ResourceCommand {
  /// Remove existing resource
  #[clap(alias("rm"))]
  Remove(ResourceRemoveOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List(ResourceListOpts),
  /// Inspect a resource
  Inspect(ResourceInspectOpts),
  /// Browse history of a resource
  History(ResourceHistoryOpts),
  /// Revert a resource to a specific history
  Revert(ResourceRevertOpts),
}

/// ## ResourceListOpts
///
/// `nanocl resource list` available options
///
#[derive(Debug, Parser)]
pub struct ResourceListOpts {
  /// Show only resource names
  #[clap(long, short)]
  pub quiet: bool,
}

/// ## ResourceArg
///
/// `nanocl resource` available arguments
///
#[derive(Debug, Parser)]
#[clap(name = "nanocl-resource")]
pub struct ResourceArg {
  #[clap(subcommand)]
  pub command: ResourceCommand,
}

/// ## ResourceRow
///
/// A row of the resource table
///
#[derive(Debug, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ResourceRow {
  /// Name of the resource
  pub name: String,
  /// Kind of resource
  pub kind: String,
  /// Version of the ressource
  pub version: String,
  /// When the resource was created
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  /// When the resource was updated
  #[tabled(rename = "UPDATED AT")]
  pub updated_at: String,
}

impl From<Resource> for ResourceRow {
  fn from(resource: Resource) -> Self {
    // Get the current timezone
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(resource.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(resource.updated_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");

    Self {
      name: resource.name,
      version: resource.version,
      kind: resource.kind,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

/// ## ResourceRemoveOpts
///
/// `nanocl resource remove` available options
///
#[derive(Debug, Parser)]
pub struct ResourceRemoveOpts {
  /// Skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// The names of the resources to delete
  pub names: Vec<String>,
}

/// ## ResourceInspectOpts
///
/// `nanocl resource inspect` available options
///
#[derive(Clone, Debug, Parser)]
pub struct ResourceInspectOpts {
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
  /// The name of the resource to inspect
  pub name: String,
}

/// ## ResourceHistoryOpts
///
/// `nanocl resource history` available options
///
#[derive(Debug, Parser)]
pub struct ResourceHistoryOpts {
  /// The name of the resource to browse history
  pub name: String,
}

/// ## ResourceRevertOpts
///
/// `nanocl resource revert` available options
///
#[derive(Debug, Parser)]
pub struct ResourceRevertOpts {
  /// The name of the resource to revert
  pub name: String,
  /// The key of the history to revert to
  pub key: String,
}
