use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::resource::Resource;

use super::DisplayFormat;

/// Resource commands
#[derive(Debug, Subcommand)]
pub enum ResourceCommands {
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

#[derive(Debug, Parser)]
pub struct ResourceListOpts {
  /// Show only resource names
  #[clap(long, short)]
  pub quiet: bool,
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
  pub kind: String,
  pub config_version: String,
  pub created_at: String,
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
      config_version: resource.version,
      kind: resource.kind,
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

#[derive(Debug, Parser)]
pub struct ResourceRemoveOpts {
  /// Skip confirmation
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// The names of the resources to delete
  pub names: Vec<String>,
}

#[derive(Clone, Debug, Parser)]
pub struct ResourceInspectOpts {
  /// Display format
  #[clap(long)]
  pub display: Option<DisplayFormat>,
  /// The name of the resource to inspect
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct ResourceHistoryOpts {
  /// The name of the resource to browse history
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct ResourceRevertOpts {
  /// The name of the resource to revert
  pub name: String,
  /// The key of the history to revert to
  pub key: String,
}
