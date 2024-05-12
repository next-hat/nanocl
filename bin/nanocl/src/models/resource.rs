use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::resource::Resource;

use super::{GenericInspectOpts, GenericListOpts, GenericRemoveOpts};

/// `nanocl resource` available commands
#[derive(Clone, Subcommand)]
pub enum ResourceCommand {
  /// Remove existing resource
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts),
  /// List existing namespaces
  #[clap(alias("ls"))]
  List(GenericListOpts),
  /// Inspect a resource
  Inspect(GenericInspectOpts),
  /// Browse history of a resource
  History(ResourceHistoryOpts),
  /// Revert a resource to a specific history
  Revert(ResourceRevertOpts),
}

/// `nanocl resource` available arguments
#[derive(Clone, Parser)]
#[clap(name = "nanocl-resource")]
pub struct ResourceArg {
  #[clap(subcommand)]
  pub command: ResourceCommand,
}

/// A row of the resource table
#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ResourceRow {
  /// Name of the resource
  pub name: String,
  /// Kind of resource
  pub kind: String,
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
      .timestamp_opt(resource.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(resource.spec.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: resource.spec.resource_key,
      kind: format!("{}/{}", resource.kind, resource.spec.version),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

/// `nanocl resource history` available options
#[derive(Clone, Parser)]
pub struct ResourceHistoryOpts {
  /// The name of the resource to browse history
  pub name: String,
}

/// `nanocl resource revert` available options
#[derive(Clone, Parser)]
pub struct ResourceRevertOpts {
  /// The name of the resource to revert
  pub name: String,
  /// The key of the history to revert to
  pub key: String,
}
