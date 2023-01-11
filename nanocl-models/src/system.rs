#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use super::cargo::CargoInspect;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Version {
  pub arch: String,
  pub commit_id: String,
  pub version: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum Event {
  NamespaceCreated(String),
  CargoCreated(Box<CargoInspect>),
  CargoDeleted(String),
  CargoStarted(Box<CargoInspect>),
  CargoStopped(Box<CargoInspect>),
  CargoPatched(Box<CargoInspect>),
}
