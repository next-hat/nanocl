#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use bollard_next::{
  container::Config,
  models::{HealthConfig, HostConfig, PortBinding, PortMap},
};

#[cfg(feature = "utoipa")]
use super::generic::Any;

use crate::generic::ImagePullPolicy;

/// Docker network mode used by a Cargo.
///
/// Docker accepts any non-blank network name in addition to its built-in
/// modes. Nanocl reserves `container:<id>` because users must not join a
/// container-owned network namespace directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct CargoNetworkMode(String);

impl CargoNetworkMode {
  /// Create a Cargo network mode from a Docker network name or built-in mode.
  pub fn new(value: impl Into<String>) -> Result<Self, CargoNetworkModeError> {
    let value = value.into();
    if value.trim().is_empty() {
      return Err(CargoNetworkModeError::Empty);
    }
    if value.starts_with("container:") {
      return Err(CargoNetworkModeError::ContainerNamespace);
    }
    Ok(Self(value))
  }

  /// Return the Docker network mode as scalar text.
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Consume the mode and return its scalar representation.
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl std::fmt::Display for CargoNetworkMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.as_str())
  }
}

impl std::str::FromStr for CargoNetworkMode {
  type Err = CargoNetworkModeError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl TryFrom<String> for CargoNetworkMode {
  type Error = CargoNetworkModeError;

  fn try_from(value: String) -> Result<Self, Self::Error> {
    Self::new(value)
  }
}

impl AsRef<str> for CargoNetworkMode {
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for CargoNetworkMode {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    Self::new(value).map_err(serde::de::Error::custom)
  }
}

/// Error returned for a Cargo network mode Nanocl cannot safely own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoNetworkModeError {
  /// Docker network names must contain at least one non-whitespace character.
  Empty,
  /// Joining another container's network namespace is reserved for Nanocl.
  ContainerNamespace,
}

impl std::fmt::Display for CargoNetworkModeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty => f.write_str("Cargo network mode cannot be blank"),
      Self::ContainerNamespace => f.write_str(
        "Cargo network mode cannot use a container namespace reference",
      ),
    }
  }
}

impl std::error::Error for CargoNetworkModeError {}

/// Strict Serde adapters for generated Bollard models.
///
/// Bollard's generated models intentionally ignore unknown fields. These
/// adapters retain those exact models while turning every ignored nested key
/// into a deserialization error.
#[cfg(feature = "serde")]
pub mod strict_bollard {
  use super::{Config, PortMap};

  fn push_path(path: &serde_ignored::Path<'_>, segments: &mut Vec<String>) {
    match path {
      serde_ignored::Path::Root => {}
      serde_ignored::Path::Seq { parent, index } => {
        push_path(parent, segments);
        segments.push(index.to_string());
      }
      serde_ignored::Path::Map { parent, key } => {
        push_path(parent, segments);
        segments.push(key.clone());
      }
      serde_ignored::Path::Some { parent }
      | serde_ignored::Path::NewtypeStruct { parent }
      | serde_ignored::Path::NewtypeVariant { parent } => {
        push_path(parent, segments);
      }
    }
  }

  fn render_path(path: &serde_ignored::Path<'_>) -> String {
    let mut segments = Vec::new();
    push_path(path, &mut segments);
    segments.join(".")
  }

  fn deserialize_strict<'de, D, T>(
    deserializer: D,
    root: &str,
  ) -> Result<T, D::Error>
  where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
  {
    let mut unknown = Vec::new();
    let value = serde_ignored::deserialize(deserializer, |path| {
      let path = render_path(&path);
      unknown.push(if path.is_empty() {
        root.to_owned()
      } else {
        format!("{root}.{path}")
      });
    })?;
    unknown.sort_unstable();
    unknown.dedup();

    if unknown.is_empty() {
      Ok(value)
    } else {
      Err(serde::de::Error::custom(format!(
        "unknown Bollard field(s): {}",
        unknown.join(", ")
      )))
    }
  }

  /// Deserialize a Bollard container [`Config`] without ignored fields.
  pub fn deserialize_config<'de, D>(deserializer: D) -> Result<Config, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "Container")
  }

  /// Deserialize a Bollard [`PortMap`] without ignored binding fields.
  pub fn deserialize_port_map<'de, D>(
    deserializer: D,
  ) -> Result<PortMap, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "PortBindings")
  }

  /// Deserialize an optional Bollard [`PortMap`] without ignored binding
  /// fields.
  pub fn deserialize_optional_port_map<'de, D>(
    deserializer: D,
  ) -> Result<Option<PortMap>, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    deserialize_strict(deserializer, "PortBindings")
  }
}

#[cfg(feature = "serde")]
fn default_container_essential() -> bool {
  true
}

/// Declaration of one named container in a future multi-container Cargo.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct ContainerSpec {
  /// Name unique within the Cargo declaration.
  pub name: String,
  /// Whether failure of this container makes the Cargo replica unhealthy.
  #[cfg_attr(
    feature = "serde",
    serde(default = "default_container_essential")
  )]
  pub essential: bool,
  /// Secrets injected into only this container.
  #[cfg_attr(feature = "serde", serde(default))]
  pub secrets: Vec<String>,
  /// Secret used to pull this container's image.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Policy used to pull this container's image.
  #[cfg_attr(feature = "serde", serde(default))]
  pub image_pull_policy: ImagePullPolicy,
  /// Exact Docker container configuration.
  #[cfg_attr(
    feature = "serde",
    serde(deserialize_with = "strict_bollard::deserialize_config")
  )]
  pub container: Config,
}

/// Pure validation error for a [`ContainerSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerSpecValidationError {
  /// The required container name was empty.
  EmptyName,
  /// The Bollard container config did not declare an image.
  MissingImage,
  /// The Bollard container config declared an empty image.
  EmptyImage,
  /// The raw Bollard network mode was not omitted, `host`, or `none`.
  InvalidNetworkMode,
  /// A field reserved for the future Cargo compiler was present.
  ForbiddenField(&'static str),
  /// A raw reference to another container-owned namespace was present.
  ContainerNamespaceReference(&'static str),
}

impl std::fmt::Display for ContainerSpecValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::EmptyName => f.write_str("ContainerSpec.Name cannot be empty"),
      Self::MissingImage => {
        f.write_str("ContainerSpec.Container.Image is required")
      }
      Self::EmptyImage => {
        f.write_str("ContainerSpec.Container.Image cannot be empty")
      }
      Self::InvalidNetworkMode => f.write_str(
        "Container.HostConfig.NetworkMode must be omitted, \"host\", or \"none\"",
      ),
      Self::ForbiddenField(field) => {
        write!(f, "ContainerSpec.{field} is owned by the Cargo compiler")
      }
      Self::ContainerNamespaceReference(field) => write!(
        f,
        "ContainerSpec.{field} cannot reference another container namespace"
      ),
    }
  }
}

impl std::error::Error for ContainerSpecValidationError {}

impl ContainerSpec {
  /// Validate the declaration without consulting Docker or daemon state.
  pub fn validate(&self) -> Result<(), ContainerSpecValidationError> {
    if self.name.is_empty() {
      return Err(ContainerSpecValidationError::EmptyName);
    }

    match self.container.image.as_deref() {
      None => return Err(ContainerSpecValidationError::MissingImage),
      Some("") => return Err(ContainerSpecValidationError::EmptyImage),
      Some(_) => {}
    }

    if self.container.networking_config.is_some() {
      return Err(ContainerSpecValidationError::ForbiddenField(
        "Container.NetworkingConfig",
      ));
    }
    if self.container.network_disabled.is_some() {
      return Err(ContainerSpecValidationError::ForbiddenField(
        "Container.NetworkDisabled",
      ));
    }

    let Some(host_config) = self.container.host_config.as_ref() else {
      return Ok(());
    };

    if host_config
      .network_mode
      .as_deref()
      .is_some_and(|mode| !matches!(mode, "host" | "none"))
    {
      return Err(ContainerSpecValidationError::InvalidNetworkMode);
    }

    for (present, field) in [
      (
        host_config.port_bindings.is_some(),
        "Container.HostConfig.PortBindings",
      ),
      (
        host_config.publish_all_ports.is_some(),
        "Container.HostConfig.PublishAllPorts",
      ),
      (
        host_config.auto_remove.is_some(),
        "Container.HostConfig.AutoRemove",
      ),
    ] {
      if present {
        return Err(ContainerSpecValidationError::ForbiddenField(field));
      }
    }

    for (mode, field) in [
      (
        host_config.ipc_mode.as_deref(),
        "Container.HostConfig.IpcMode",
      ),
      (
        host_config.pid_mode.as_deref(),
        "Container.HostConfig.PidMode",
      ),
      // Docker's CgroupSpec uses container:<id> to select another
      // container's cgroup even though CgroupnsMode itself is a closed enum.
      (host_config.cgroup.as_deref(), "Container.HostConfig.Cgroup"),
      (
        host_config.uts_mode.as_deref(),
        "Container.HostConfig.UTSMode",
      ),
      (
        host_config.userns_mode.as_deref(),
        "Container.HostConfig.UsernsMode",
      ),
    ] {
      if mode.is_some_and(|mode| mode.starts_with("container:")) {
        return Err(ContainerSpecValidationError::ContainerNamespaceReference(
          field,
        ));
      }
    }

    Ok(())
  }
}

/// Enum to define the strategy to place the cargo on the nodes
///
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", untagged)
)]
pub enum CargoPlacementStrategy {
  Distinct,
  LeastLoaded,
  Balanced,
  UserDefined(String),
}

/// Resource requirements for the cargo
///
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct CargoResourceRequirementSpec {
  /// CPU requirement in number of cores
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_cores: Option<usize>,
  /// Maximum allowed average CPU utilization on a target node (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_utilization_cap: Option<f32>,
  /// Memory requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_bytes: Option<usize>,
  /// Maximum allowed average Memory utilization on a target node (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_utilization_cap: Option<f32>,
  /// Optional weight for CPU in balanced placement (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cpu_weight: Option<f32>,
  /// Optional weight for Memory in balanced placement (0.0 - 1.0)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub memory_weight: Option<f32>,
  /// Storage requirement in bytes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub storage_bytes: Option<usize>,
}

/// Generic constraint operator for node selection
/// Allows advanced matching beyond simple selectors
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub enum ConstraintOp {
  Eq,
  Ne,
}

/// Arbitrary node constraint expressed as key/operator/value
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct NodeConstraint {
  /// Constraint key (eg: "region", "label", "group", etc.)
  pub key: String,
  /// Constraint operator (Eq/Ne)
  pub operator: ConstraintOp,
  /// Constraint value
  pub value: String,
}

/// Way to select specific or preferred nodes to place the cargo
///
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", default)
)]
pub struct CargoPlacementSelectorSpec {
  /// Select by labels
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub labels: Option<Vec<String>>,
  /// Select by regions
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// Select by groups
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub groups: Option<Vec<String>>,
  /// Select by cargoes running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub cargoes: Option<Vec<String>>,
  /// Select by vms running on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub vms: Option<Vec<String>>,
  /// Optional advanced constraints (key/operator/value)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub constraints: Option<Vec<NodeConstraint>>,
}

/// Structure to control the placement of the cargo
/// It can be used to define on which nodes the cargo will be deployed
///
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", default)
)]
pub struct CargoPlacementSpec {
  /// Number of replicas to deploy on the selected nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub replicas: Option<usize>,
  /// Strategy to use to place the cargo on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub strategy: Option<CargoPlacementStrategy>,
  /// Regions to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub regions: Option<Vec<String>>,
  /// requirements to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub require: Option<CargoPlacementSelectorSpec>,
  /// preferences to select nodes to deploy the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub prefer: Option<CargoPlacementSelectorSpec>,
}

/// A cargo spec partial is used to create a Cargo
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", default)
)]
pub struct CargoSpecPartial {
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

/// Payload used to patch a cargo
/// It will create a new [CargoSpec](CargoSpec) with the new values
/// It will keep the old values in the history
///
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase", default)
)]
pub struct CargoSpecUpdate {
  /// New name of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub name: Option<String>,
  /// New metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// New container specification of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub container: Option<Config>,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  /// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

impl From<CargoSpecPartial> for CargoSpecUpdate {
  fn from(spec: CargoSpecPartial) -> Self {
    Self {
      name: Some(spec.name),
      init_container: spec.init_container,
      container: Some(spec.container),
      placement: spec.placement,
      resource_requirement: spec.resource_requirement,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}

/// A cargo spec is the specification of a cargo
/// It used to know the state of the cargo
/// It keep tracking of an history when you patch an existing cargo
///
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct CargoSpec {
  /// Unique identifier of the cargo spec
  pub key: uuid::Uuid,
  /// Canonical cargo key in `{namespace}.{name}` format
  pub cargo_key: String,
  /// Version of the spec
  pub version: String,
  /// Creation date of the cargo spec
  pub created_at: chrono::NaiveDateTime,
  /// Name of the cargo
  pub name: String,
  /// Metadata of the cargo (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Action to run before the container
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub init_container: Option<Config>,
  /// List of secrets to use as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy of the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Container specification of the cargo
  pub container: Config,
  /// Fine tune how the cargo is placed on the nodes
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub placement: Option<CargoPlacementSpec>,
  //// Define minimal resource requirements for the cargo
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

impl From<CargoSpec> for CargoSpecPartial {
  fn from(spec: CargoSpec) -> Self {
    Self {
      init_container: spec.init_container,
      name: spec.name,
      placement: spec.placement,
      resource_requirement: spec.resource_requirement,
      container: spec.container,
      metadata: spec.metadata,
      secrets: spec.secrets,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}
