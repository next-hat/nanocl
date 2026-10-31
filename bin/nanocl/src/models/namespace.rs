use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

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
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  #[tabled(skip)]
  pub age: String,
}

/// A compact row of the namespace table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NamespaceCompactRow {
  pub name: String,
  pub cargoes: usize,
  pub instances: usize,
  pub age: String,
}

impl From<NamespaceRow> for NamespaceCompactRow {
  fn from(row: NamespaceRow) -> Self {
    Self {
      name: row.name,
      cargoes: row.cargoes,
      instances: row.instances,
      age: row.age,
    }
  }
}

/// Convert a NamespaceSummary to a NamespaceRow
impl From<NamespaceSummary> for NamespaceRow {
  fn from(item: NamespaceSummary) -> Self {
    let age =
      super::format_age(Some(&item.created_at.and_utc()), chrono::Utc::now());
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
      created_at: created_at.to_string(),
      age,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::commands::GenericCommandLs;

  #[test]
  fn namespace_list_compact_wide_and_quiet() {
    let created_at =
      (chrono::Utc::now() - chrono::Duration::days(3)).naive_utc();
    let fixture = || NamespaceSummary {
      name: "production".to_owned(),
      cargoes: 7,
      instances: 12,
      created_at,
    };
    let row = NamespaceRow::from(fixture());
    assert_eq!(row.age, "3d");
    let exact_created_at = row.created_at.clone();
    let mut opts =
      GenericListOpts::<super::super::GenericDefaultOpts>::default();
    let compact = NamespaceArg::render_list(&opts, vec![row]);
    assert_eq!(
      NamespaceCompactRow::headers(),
      ["NAME", "CARGOES", "INSTANCES", "AGE"]
    );
    assert!(compact.contains("production"));
    assert!(compact.contains("3d"));
    assert!(!compact.contains("CREATED AT"));
    assert!(!compact.contains(&exact_created_at));

    opts.wide = true;
    let wide =
      NamespaceArg::render_list(&opts, vec![NamespaceRow::from(fixture())]);
    assert!(wide.contains("CREATED AT"));
    assert!(wide.contains(&exact_created_at));
    assert!(!NamespaceRow::headers().iter().any(|header| header == "AGE"));

    opts.quiet = true;
    assert_eq!(
      NamespaceArg::render_list(&opts, vec![NamespaceRow::from(fixture())]),
      "production"
    );
  }
}
