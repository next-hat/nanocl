use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

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
  #[tabled(skip)]
  pub age: String,
}

/// A compact row of the resource table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ResourceCompactRow {
  pub name: String,
  pub kind: String,
  pub age: String,
}

impl From<ResourceRow> for ResourceCompactRow {
  fn from(row: ResourceRow) -> Self {
    Self {
      name: row.name,
      kind: row.kind,
      age: row.age,
    }
  }
}

impl From<Resource> for ResourceRow {
  fn from(resource: Resource) -> Self {
    let age = super::format_age(
      Some(&resource.created_at.and_utc()),
      chrono::Utc::now(),
    );
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
      age,
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::GenericCommandLs;
  use nanocld_client::stubs::resource::ResourceSpec;

  #[test]
  fn resource_list_compact_wide_and_quiet() {
    let created_at =
      (chrono::Utc::now() - chrono::Duration::days(3)).naive_utc();
    let updated_at =
      (chrono::Utc::now() - chrono::Duration::hours(1)).naive_utc();
    let fixture = || Resource {
      kind: "ProxyRule".to_owned(),
      created_at,
      spec: ResourceSpec {
        key: uuid::Uuid::nil(),
        version: "v1".to_owned(),
        created_at: updated_at,
        resource_key: "web-route".to_owned(),
        data: serde_json::json!({}),
        metadata: None,
      },
    };
    let row = ResourceRow::from(fixture());
    assert_eq!(row.age, "3d");
    let exact_created_at = row.created_at.clone();
    let exact_updated_at = row.updated_at.clone();
    let mut opts =
      GenericListOpts::<super::super::GenericDefaultOpts>::default();
    let compact = ResourceArg::render_list(&opts, vec![row]);
    assert_eq!(ResourceCompactRow::headers(), ["NAME", "KIND", "AGE"]);
    assert!(compact.contains("web-route"));
    assert!(compact.contains("ProxyRule/v1"));
    assert!(compact.contains("3d"));
    assert!(!compact.contains("CREATED AT"));
    assert!(!compact.contains("UPDATED AT"));

    opts.wide = true;
    let wide =
      ResourceArg::render_list(&opts, vec![ResourceRow::from(fixture())]);
    assert!(wide.contains("CREATED AT"));
    assert!(wide.contains("UPDATED AT"));
    assert!(wide.contains(&exact_created_at));
    assert!(wide.contains(&exact_updated_at));

    opts.quiet = true;
    assert_eq!(
      ResourceArg::render_list(&opts, vec![ResourceRow::from(fixture())]),
      "web-route"
    );
  }
}
