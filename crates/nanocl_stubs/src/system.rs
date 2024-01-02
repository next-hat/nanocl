use std::str::FromStr;

use bollard_next::service::SystemInfo;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::config::DaemonConfig;

/// HostInfo contains information about the host and the docker daemon
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct HostInfo {
  /// Docker contains information about the docker daemon
  #[cfg_attr(feature = "serde", serde(flatten))]
  pub docker: SystemInfo,
  /// HostGateway is the gateway address of the host
  pub host_gateway: String,
  /// Daemon configuration
  pub config: DaemonConfig,
}

/// Details about the binary
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct BinaryInfo {
  /// Arch is the architecture of the current binary
  pub arch: String,
  /// Channel is the channel of the current binary
  pub channel: String,
  /// Version is the version of the current binary
  pub version: String,
  /// CommitID is the commit id of the current binary
  pub commit_id: String,
}

/// Kind is the type of event related to the actor kind
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum EventActorKind {
  Namespace,
  Cargo,
  Vm,
  Job,
  Resource,
  Secret,
  Process,
}

impl std::fmt::Display for EventActorKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EventActorKind::Namespace => write!(f, "Namespace"),
      EventActorKind::Cargo => write!(f, "Cargo"),
      EventActorKind::Vm => write!(f, "Vm"),
      EventActorKind::Job => write!(f, "Job"),
      EventActorKind::Resource => write!(f, "Resource"),
      EventActorKind::Secret => write!(f, "Secret"),
      EventActorKind::Process => write!(f, "Process"),
    }
  }
}

/// Action is the action that triggered the event
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum NativeEventAction {
  Create,
  Patch,
  Start,
  Stop,
  Delete,
  Restart,
  Other(String),
}

impl FromStr for NativeEventAction {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "create" => Ok(NativeEventAction::Create),
      "patch" => Ok(NativeEventAction::Patch),
      "start" => Ok(NativeEventAction::Start),
      "stop" => Ok(NativeEventAction::Stop),
      "delete" => Ok(NativeEventAction::Delete),
      "restart" => Ok(NativeEventAction::Restart),
      _ => Ok(NativeEventAction::Other(s.to_owned())),
    }
  }
}

impl std::fmt::Display for NativeEventAction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      NativeEventAction::Create => write!(f, "create"),
      NativeEventAction::Patch => write!(f, "patch"),
      NativeEventAction::Start => write!(f, "start"),
      NativeEventAction::Stop => write!(f, "stop"),
      NativeEventAction::Delete => write!(f, "delete"),
      NativeEventAction::Restart => write!(f, "restart"),
      NativeEventAction::Other(s) => write!(f, "{}", s),
    }
  }
}

/// Kind of event (Error, Normal, Warning), new types could be added in the future.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum EventKind {
  Error,
  Normal,
  Warning,
}

impl FromStr for EventKind {
  type Err = std::io::Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "error" => Ok(EventKind::Error),
      "normal" => Ok(EventKind::Normal),
      "warning" => Ok(EventKind::Warning),
      _ => Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("invalid event kind: {}", s),
      )),
    }
  }
}

impl std::fmt::Display for EventKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EventKind::Error => write!(f, "error"),
      EventKind::Normal => write!(f, "normal"),
      EventKind::Warning => write!(f, "warning"),
    }
  }
}

/// Actor is the actor that triggered the event
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct EventActor {
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub key: Option<String>,
  pub kind: EventActorKind,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub attributes: Option<serde_json::Value>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct EventPartial {
  /// Reporting Node is the name of the node where the Event was generated.
  pub reporting_node: String,
  /// Reporting Controller is the name of the controller that emitted this Event.
  /// e.g. `nanocl.io/core`. This field cannot be empty for new Events.
  pub reporting_controller: String,
  /// Kind of this event (Error, Normal, Warning), new types could be added in the future.
  /// It is machine-readable. This field cannot be empty for new Events.
  pub kind: EventKind,
  /// Action is what action was taken/failed regarding to the regarding actor.
  /// It is machine-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub action: String,
  /// Reason is why the action was taken. It is human-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub reason: String,
  /// Human-readable description of the status of this operation
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub note: Option<String>,
  /// Actor contains the object this Event is about.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub actor: Option<EventActor>,
  /// Optional secondary actor for more complex actions.
  /// E.g. when regarding actor triggers a creation or deletion of related actor.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub related: Option<EventActor>,
  /// Standard metadata.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}

/// Event is a generic event type that is used to notify state changes
#[derive(Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Event {
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
  pub kind: EventKind,
  /// Action is what action was taken/failed regarding to the regarding actor.
  /// It is machine-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub action: String,
  /// Reason is why the action was taken. It is human-readable.
  /// This field cannot be empty for new Events and it can have at most 128 characters.
  pub reason: String,
  /// Human-readable description of the status of this operation
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub note: Option<String>,
  /// Actor contains the object this Event is about.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub actor: Option<EventActor>,
  /// Optional secondary actor for more complex actions.
  /// E.g. when regarding actor triggers a creation or deletion of related actor.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub related: Option<EventActor>,
  /// Standard metadata.
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub metadata: Option<serde_json::Value>,
}
