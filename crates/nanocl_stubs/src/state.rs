#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::cargo_config::CargoConfigPartial;
use crate::vm_config::VmConfigPartial;

use super::resource::ResourcePartial;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateMeta {
  pub api_version: String,
  pub kind: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateResource {
  pub resources: Vec<ResourcePartial>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateCargo {
  pub namespace: Option<String>,
  pub cargoes: Vec<CargoConfigPartial>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateVirtualMachine {
  pub namespace: Option<String>,
  pub virtual_machines: Vec<VmConfigPartial>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateDeployment {
  pub namespace: Option<String>,
  pub resources: Option<Vec<ResourcePartial>>,
  pub cargoes: Option<Vec<CargoConfigPartial>>,
  pub virtual_machines: Option<Vec<VmConfigPartial>>,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum StateStreamStatus {
  Pending,
  Failed,
  Success,
  NotFound,
  UnChanged,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum StateStreamKind {
  Cargo,
  VirtualMachine,
  Resource,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateStream {
  pub key: String,
  pub kind: String,
  pub context: Option<String>,
  pub status: StateStreamStatus,
}

impl StateStream {
  pub fn new_cargo_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_string(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_cargo_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_string(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_cargo_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_string(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_cargo_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_string(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_cargo_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_string(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_vm_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_string(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_vm_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_string(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_vm_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_string(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_vm_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_string(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_vm_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_string(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_resource_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_string(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_resource_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_string(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_resource_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_string(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_resource_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_string(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_resource_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_string(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }
}
