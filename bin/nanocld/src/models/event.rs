use std::str::FromStr;

use diesel::prelude::*;

use nanocl_error::io::IoError;
use nanocl_stubs::system::{Event, EventKind, EventPartial};

use crate::schema::events;

#[derive(Debug, Queryable, Identifiable, Insertable)]
#[diesel(primary_key(key))]
#[diesel(table_name = events)]
pub struct EventDb {
  /// Unique identifier of this event.
  pub key: uuid::Uuid,
  /// When the event was created.
  pub created_at: chrono::NaiveDateTime,
  /// When the event expires.
  pub expires_at: chrono::NaiveDateTime,
  /// Reporting Node is the name of the node where the Event was generated.
  pub reporting_node: String,
  /// Reporting Controller is the name of the controller that emitted this Event.
  /// e.g. `nanocl.io/core`. This field cannot be empty for new Events.
  pub reporting_controller: String,
  /// Kind of this event (Error, Normal, Warning), new types could be added in the future.
  /// It is machine-readable. This field cannot be empty for new Events.
  pub kind: String,
  /// Action is what action was taken/failed regarding to the regarding actor.
  /// It is machine-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub action: String,
  /// Reason is why the action was taken. It is human-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub reason: String,
  /// Human-readable description of the status of this operation
  pub note: Option<String>,
  /// Actor contains the object this Event is about.
  pub actor: Option<serde_json::Value>,
  /// Optional secondary actor for more complex actions.
  /// E.g. when regarding actor triggers a creation or deletion of related actor.
  pub related: Option<serde_json::Value>,
  /// Standard metadata.
  pub metadata: Option<serde_json::Value>,
}

impl TryFrom<EventPartial> for EventDb {
  type Error = IoError;

  fn try_from(value: EventPartial) -> Result<Self, Self::Error> {
    Ok(EventDb {
      key: uuid::Uuid::new_v4(),
      created_at: chrono::Utc::now().naive_utc(),
      expires_at: chrono::Utc::now().naive_utc()
        + chrono::Duration::try_days(30).unwrap(),
      reporting_node: value.reporting_node,
      reporting_controller: value.reporting_controller,
      kind: value.kind.to_string(),
      action: value.action,
      reason: value.reason,
      note: value.note,
      actor: value.actor.map(serde_json::to_value).transpose()?,
      related: value.related.map(serde_json::to_value).transpose()?,
      metadata: value.metadata.map(serde_json::to_value).transpose()?,
    })
  }
}

impl TryFrom<EventDb> for Event {
  type Error = IoError;

  fn try_from(value: EventDb) -> Result<Self, Self::Error> {
    Ok(Event {
      key: value.key,
      created_at: value.created_at,
      expires_at: value.expires_at,
      reporting_node: value.reporting_node,
      reporting_controller: value.reporting_controller,
      kind: EventKind::from_str(&value.kind)?,
      action: value.action,
      reason: value.reason,
      note: value.note,
      actor: value.actor.map(serde_json::from_value).transpose()?,
      related: value.related.map(serde_json::from_value).transpose()?,
      metadata: value.metadata.map(serde_json::from_value).transpose()?,
    })
  }
}
