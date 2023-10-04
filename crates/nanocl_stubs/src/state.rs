#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::secret::SecretPartial;
use crate::vm_config::VmConfigPartial;
use crate::cargo_config::CargoConfigPartial;

use super::resource::ResourcePartial;

/// ## StateMeta
///
/// Statefile metadata information that are always present
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateMeta {
  /// Api version to use or remote url
  pub api_version: String,
  /// Kind of Statefile (Deployment, Cargo, VirtualMachine, Resource)
  pub kind: String,
}

/// ## StateResource
///
/// Statefile that represent the `Resource` kind
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateResource {
  /// List of resources to create
  pub resources: Vec<ResourcePartial>,
}

/// ## StateSecret
///
/// Statefile that represent the `Secret` kind
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateSecret {
  /// List of secrets to create
  pub secrets: Vec<SecretPartial>,
}

/// ## StateCargo
///
/// Statefile that represent the `Cargo` kind
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateCargo {
  /// Namespace where the cargoes are deployed
  pub namespace: Option<String>,
  /// List of cargoes to create and run
  pub cargoes: Vec<CargoConfigPartial>,
}

/// ## StateVirtualMachine
///
/// Statefile that represent the `VirtualMachine` kind
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateVirtualMachine {
  /// Namespace where the virtual machines are deployed
  pub namespace: Option<String>,
  /// List of virtual machines to create and run
  pub virtual_machines: Vec<VmConfigPartial>,
}

/// ## StateDeployment
///
/// Statefile that represent the `Deployment` kind
///
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateDeployment {
  /// Namespace where the cargoes and virtual machines are deployed
  pub namespace: Option<String>,
  /// List of resources to create
  pub resources: Option<Vec<ResourcePartial>>,
  /// List of secrets to create
  pub secrets: Option<Vec<SecretPartial>>,
  /// List of cargoes to create and run
  pub cargoes: Option<Vec<CargoConfigPartial>>,
  /// List of virtual machines to create and run
  pub virtual_machines: Option<Vec<VmConfigPartial>>,
}

/// ## StateStreamStatus
///
/// Status of a apply status for a cargo or a virtual machine
///
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum StateStreamStatus {
  /// The cargo or virtual machine is pending to be applied or removed
  Pending,
  /// The cargo or virtual machine is failed to be applied or removed
  Failed,
  /// The cargo or virtual machine is successfull applied or removed
  Success,
  /// The cargo or virtual machine is not found remove is skipped
  NotFound,
  /// The cargo or virtual machine is not changed apply is skipped
  UnChanged,
}

/// ## StateStreamKind
///
/// Kind of stream that is used to apply a cargo or a virtual machine
/// This is used to know if the status of an applied or removed cargo or virtual machine
///
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum StateStreamKind {
  /// The stream is used to apply or remove a cargo
  Cargo,
  /// The stream is used to apply or remove a virtual machine
  VirtualMachine,
  /// The stream is used to apply or remove a resource
  Resource,
}

/// ## StateStream
///
/// Stream that represent the status of the apply or remove of a cargo or a virtual machine
///
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateStream {
  /// The key of the element (Cargo, VirtualMachine, Resource)
  pub key: String,
  /// The kind of the element (Cargo, VirtualMachine, Resource)
  pub kind: String,
  /// Some context information about the status
  pub context: Option<String>,
  /// The status of the element (Pending, Failed, Success, NotFound, UnChanged)
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

  pub fn new_secret_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_string(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_secret_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_string(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_secret_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_string(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_secret_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_string(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_secret_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_string(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }
}
