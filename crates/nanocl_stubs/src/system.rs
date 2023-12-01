use bollard_next::service::SystemInfo;
use bollard_next::container::ListContainersOptions;

#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::config::DaemonConfig;

/// HostInfo contains information about the host and the docker daemon
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum EventKind {
  Namespace,
  Cargo,
  Vm,
  Job,
  Resource,
  Secret,
  Container,
}

impl std::fmt::Display for EventKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EventKind::Namespace => write!(f, "Namespace"),
      EventKind::Cargo => write!(f, "Cargo"),
      EventKind::Vm => write!(f, "Vm"),
      EventKind::Job => write!(f, "Job"),
      EventKind::Resource => write!(f, "Resource"),
      EventKind::Secret => write!(f, "Secret"),
      EventKind::Container => write!(f, "ContainerInstance"),
    }
  }
}

/// Action is the action that triggered the event
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum EventAction {
  Created,
  Patched,
  Started,
  Stopped,
  Deleted,
  Restart,
}

impl std::fmt::Display for EventAction {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EventAction::Created => write!(f, "Created"),
      EventAction::Patched => write!(f, "Patched"),
      EventAction::Started => write!(f, "Started"),
      EventAction::Stopped => write!(f, "Stopped"),
      EventAction::Deleted => write!(f, "Deleted"),
      EventAction::Restart => write!(f, "Restart"),
    }
  }
}

/// Actor is the actor that triggered the event
#[derive(Default, Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct EventActor {
  pub key: Option<String>,
  pub attributes: Option<serde_json::Value>,
}

/// Event is a generic event type that is used to notify state changes
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Event {
  /// Kind of event
  pub kind: EventKind,
  /// Action of event
  pub action: EventAction,
  /// Actor of event
  pub actor: Option<EventActor>,
}

/// Generic trait to convert a type to an event
pub trait ToEvent {
  fn to_event(&self, action: EventAction) -> Event;
}

/// Query parameters for the process list endpoint.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProccessQuery {
  /// Return container from all nodes
  pub all: bool,
  /// Return this number of most recently created containers
  pub last: Option<isize>,
  /// Show all containers running for the given namespace
  pub namespace: Option<String>,
}

impl From<ProccessQuery> for ListContainersOptions<String> {
  fn from(query: ProccessQuery) -> Self {
    ListContainersOptions {
      all: query.all,
      limit: query.last,
      ..Default::default()
    }
  }
}
