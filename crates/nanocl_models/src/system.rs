#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use super::cargo::CargoInspect;
use super::resource::Resource;

/// Version contain details about the current version nanocl
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Version {
  pub arch: String,
  pub commit_id: String,
  pub version: String,
}

/// Event is a message sent by nanocld to connected clients
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum Event {
  /// NamespaceCreated is sent when a namespace is created
  NamespaceCreated(String),
  /// NamespaceDeleted is sent when a namespace is deleted
  CargoCreated(Box<CargoInspect>),
  /// CargoDeleted is sent when a cargo is deleted
  CargoDeleted(String),
  /// CargoStarted is sent when a cargo is started
  CargoStarted(Box<CargoInspect>),
  /// CargoStopped is sent when a cargo is stopped
  CargoStopped(Box<CargoInspect>),
  /// CargoPatched is sent when a cargo is patched
  CargoPatched(Box<CargoInspect>),
  /// ResourceCreated is sent when a resource is created
  ResourceCreated(Box<Resource>),
  /// ResourceDeleted is sent when a resource is deleted
  ResourceDeleted(String),
  /// ResourcePatched is sent when a resource is patched
  ResourcePatched(Box<Resource>),
}
