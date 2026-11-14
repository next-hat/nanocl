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
  Watch {
    /// Print one readable line per event
    #[arg(long)]
    plain: bool,
  },
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
  pub actor: String,
  pub reason: String,
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

impl std::fmt::Display for EventRow {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}  {}  {}  {}  ",
      self.created_at, self.node, self.kind, self.action
    )?;
    if self.actor != "<none>"
      && !self
        .note
        .strip_prefix(&self.actor)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    {
      write!(f, "{} — ", self.actor)?;
    }
    f.write_str(&self.note)
  }
}

impl From<Event> for EventRow {
  fn from(event: Event) -> Self {
    let binding = chrono::Local::now();
    let age = format_age(
      Some(&event.created_at.and_utc()),
      binding.with_timezone(&chrono::Utc),
    );
    let created_at = event.created_at.and_utc().with_timezone(&chrono::Local);
    let actor_name = event
      .actor
      .as_ref()
      .map(ToString::to_string)
      .unwrap_or_else(|| "<none>".into());
    Self {
      key: event.key.to_string(),
      created_at: created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
      kind: event.kind.to_string(),
      action: single_line(event.action),
      actor: single_line(actor_name),
      reason: single_line(event.reason),
      node: single_line(event.reporting_node),
      note: single_line(event.note.unwrap_or_else(|| "<none>".into())),
      age,
    }
  }
}

fn single_line(value: String) -> String {
  if value.contains(char::is_control) {
    value.replace(char::is_control, " ")
  } else {
    value
  }
}

#[cfg(test)]
mod tests {
  use chrono::{Duration, Utc};
  use nanocld_client::stubs::system::{EventActor, EventActorKind, EventKind};

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
    assert!(wide.contains("ACTOR"));
    assert!(wide.contains("REASON"));
    assert!(wide.contains(&row.reason));
    assert!(wide.contains("CREATED AT"));
    assert!(!wide.contains("AGE"));
    assert!(wide.contains(&row.node));
    assert!(wide.contains(&row.created_at));
    assert!(wide.contains(&row.key));
    assert!(wide.contains(&row.note));

    opts.quiet = true;
    assert_eq!(EventArg::render_list(&opts, vec![row.clone()]), row.key);
  }

  #[test]
  fn displays_daemon_notes_and_keeps_plain_output_on_one_line() {
    let mut ev = event();
    ev.actor = Some(EventActor {
      key: Some("global.api".into()),
      kind: EventActorKind::Cargo,
      attributes: None,
    });
    ev.action = "starting".into();
    ev.reason = "state_sync".into();
    ev.note = Some("Cargo global.api start requested".into());
    let row = EventRow::from(ev.clone());
    assert_eq!(row.note, "Cargo global.api start requested");
    assert_eq!(row.to_string().matches("Cargo global.api").count(), 1);
    ev.kind = EventKind::Error;
    ev.note = Some("Registry\nrequest timed out".into());
    assert!(
      EventRow::from(ev)
        .to_string()
        .ends_with("Cargo global.api — Registry request timed out")
    );
  }
}
