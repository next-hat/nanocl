use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::system::Event;

use super::{GenericInspectOpts, GenericListOpts, format_age};

#[derive(Clone, Parser)]
pub struct EventArg {
  #[clap(subcommand)]
  pub command: EventCommand,
}

/// event available commands
#[derive(Clone, Subcommand)]
pub enum EventCommand {
  /// List existing events
  #[clap(alias("ls"))]
  List(GenericListOpts),
  /// Watch for new events in real time
  Watch,
  /// Inspect a specific event
  Inspect(GenericInspectOpts),
}

#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct EventRow {
  pub key: String,
  #[tabled(rename = "CREATED AT")]
  pub created_at: String,
  pub node: String,
  pub kind: String,
  pub action: String,
  pub note: String,
  #[tabled(skip)]
  pub age: String,
}

#[derive(Clone, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct EventCompactRow {
  pub key: String,
  pub age: String,
  pub kind: String,
  pub action: String,
  pub note: String,
}

impl From<EventRow> for EventCompactRow {
  fn from(row: EventRow) -> Self {
    Self {
      key: row.key,
      age: row.age,
      kind: row.kind,
      action: row.action,
      note: row.note,
    }
  }
}

impl From<Event> for EventRow {
  fn from(event: Event) -> Self {
    let binding = chrono::Local::now();
    let age = format_age(
      Some(&event.created_at.and_utc()),
      binding.with_timezone(&chrono::Utc),
    );
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(event.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: event.key.to_string(),
      created_at: created_at.to_string(),
      kind: event.kind.to_string(),
      action: event.action,
      node: event.reporting_node,
      note: event.note.unwrap_or("<none>".to_owned()),
      age,
    }
  }
}

#[cfg(test)]
mod tests {
  use chrono::{Duration, Utc};
  use nanocld_client::stubs::system::EventKind;

  use crate::commands::GenericCommandLs;

  use super::{Event, EventArg, EventCompactRow, EventRow, GenericListOpts};

  fn event() -> Event {
    let created_at = (Utc::now() - Duration::hours(73)).naive_utc();
    Event {
      key: uuid::Uuid::from_u128(1),
      created_at,
      expires_at: created_at + Duration::days(7),
      reporting_node: "worker-a".to_owned(),
      reporting_controller: "nanocl.io/core".to_owned(),
      kind: EventKind::Normal,
      action: "Start".to_owned(),
      reason: "Requested".to_owned(),
      note: Some("Started api.global successfully".to_owned()),
      actor: None,
      related: None,
      metadata: None,
    }
  }

  #[test]
  fn event_list_age_uses_api_creation_time() {
    let wide = EventRow::from(event());
    assert_eq!(wide.age, "3d");
    let compact = EventCompactRow::from(wide.clone());
    assert_eq!(compact.age, wide.age);
    assert_eq!(compact.key, wide.key);
    assert_eq!(compact.note, wide.note);
    assert_eq!(compact.action, wide.action);
  }

  #[test]
  fn event_list_compact_wide_and_quiet_preserve_content() {
    let row = EventRow::from(event());
    let mut opts: GenericListOpts = GenericListOpts::default();
    let compact = EventArg::render_list(&opts, vec![row.clone()]);
    for header in ["KEY", "AGE", "KIND", "ACTION", "NOTE"] {
      assert!(compact.contains(header));
    }
    assert!(!compact.contains("NODE"));
    assert!(!compact.contains("CREATED AT"));
    assert!(!compact.contains(&row.node));
    assert!(!compact.contains(&row.created_at));
    assert!(compact.contains(&row.key));
    assert!(compact.contains(&row.note));
    assert!(compact.contains(&row.action));

    opts.wide = true;
    let wide = EventArg::render_list(&opts, vec![row.clone()]);
    assert!(wide.contains("NODE"));
    assert!(wide.contains("CREATED AT"));
    assert!(!wide.contains("AGE"));
    assert!(wide.contains(&row.node));
    assert!(wide.contains(&row.created_at));
    assert!(wide.contains(&row.key));
    assert!(wide.contains(&row.note));

    opts.quiet = true;
    assert_eq!(EventArg::render_list(&opts, vec![row.clone()]), row.key);
  }
}
