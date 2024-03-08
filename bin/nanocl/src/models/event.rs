use chrono::TimeZone;
use tabled::Tabled;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::system::Event;

use super::GenericListOpts;

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
}

impl From<Event> for EventRow {
  fn from(event: Event) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(event.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: event
        .key
        .to_string()
        .split('-')
        .last()
        .unwrap_or("<error>")
        .to_string(),
      created_at: created_at.to_string(),
      kind: event.kind.to_string(),
      action: event.action,
      node: event.reporting_node,
      note: event.note.unwrap_or("<none>".to_owned()),
    }
  }
}
