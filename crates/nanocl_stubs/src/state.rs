#[cfg(feature = "serde")]
use serde::{Serialize, Deserialize};

use crate::job::JobPartial;
use crate::secret::SecretPartial;
use crate::vm_spec::VmSpecPartial;
use crate::cargo_spec::CargoSpecPartial;

use super::resource::ResourcePartial;

/// Statefile argument definition to pass to the Statefile
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct StatefileArg {
  /// Name of the build arg
  pub name: String,
  /// Kind of the build arg
  pub kind: String,
  /// Default value of the build arg
  pub default: Option<String>,
}

/// Structure that represent a Statefile
#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct Statefile {
  /// Api version to use or remote url
  pub api_version: String,
  /// Arguments to pass to the Statefile
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub args: Option<Vec<StatefileArg>>,
  /// Set the group of defined objects default to `{name_of_directory}.{name_of_file}`
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub group: Option<String>,
  /// Namespace where the cargoes and virtual machines are deployed
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub namespace: Option<String>,
  /// List of secrets to create
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<SecretPartial>>,
  /// List of resources to create
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resources: Option<Vec<ResourcePartial>>,
  /// List of cargoes to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cargoes: Option<Vec<CargoSpecPartial>>,
  /// List of virtual machines to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub virtual_machines: Option<Vec<VmSpecPartial>>,
  /// List of jobs to create and run
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub jobs: Option<Vec<JobPartial>>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct StateApplyQuery {
  pub reload: Option<bool>,
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
      kind: "Cargo".to_owned(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_cargo_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_owned(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_cargo_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_owned(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_cargo_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_owned(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_cargo_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Cargo".to_owned(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_job_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Job".to_owned(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_job_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Job".to_owned(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_job_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Job".to_owned(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_job_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Job".to_owned(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_job_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Job".to_owned(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_vm_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_owned(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_vm_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_owned(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_vm_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_owned(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_vm_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_owned(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_vm_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "VirtualMachine".to_owned(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_resource_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_owned(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_resource_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_owned(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_resource_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_owned(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_resource_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_owned(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }

  pub fn new_resource_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Resource".to_owned(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_secret_error(key: &str, err: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_owned(),
      context: Some(err.to_owned()),
      status: StateStreamStatus::Failed,
    }
  }

  pub fn new_secret_pending(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_owned(),
      context: None,
      status: StateStreamStatus::Pending,
    }
  }

  pub fn new_secret_not_found(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_owned(),
      context: None,
      status: StateStreamStatus::NotFound,
    }
  }

  pub fn new_secret_unchanged(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_owned(),
      context: None,
      status: StateStreamStatus::UnChanged,
    }
  }

  pub fn new_secret_success(key: &str) -> Self {
    StateStream {
      key: key.to_owned(),
      kind: "Secret".to_owned(),
      context: None,
      status: StateStreamStatus::Success,
    }
  }
}
