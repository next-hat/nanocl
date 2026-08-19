use std::collections::HashMap;

use bollard_next::models::{EmptyObject, RestartPolicy, RestartPolicyNameEnum};
use nanocl_error::io::IoError;
use nanocl_stubs::cargo_spec::{
  CargoNetworkMode, Config as DockerConfig, ContainerSpec,
  ContainerSpecValidationError, HostConfig, PortMap,
};

use super::network::DEFAULT_NETWORK;

const INTERNAL_GATEWAY_TOKEN: &str = "$$INTERNAL_GATEWAY";
const SANDBOX_LOGICAL_NAME: &str = "_sandbox";

const LABEL_MANAGED: &str = "io.nanocl";
const LABEL_KIND: &str = "io.nanocl.kind";
const LABEL_CARGO_KEY: &str = "io.nanocl.c";
const LABEL_NAMESPACE: &str = "io.nanocl.n";
const LABEL_INIT_CONTAINER: &str = "io.nanocl.init-c";
const LABEL_APPLICATION_CONTAINER: &str = "io.nanocl.not-init-c";
const LABEL_COMPOSE_PROJECT: &str = "com.docker.compose.project";
pub(crate) const LABEL_REPLICA: &str = "io.nanocl.cargo.replica";
pub(crate) const LABEL_REPLICA_ORDINAL: &str =
  "io.nanocl.cargo.replica-ordinal";
pub(crate) const LABEL_CONTAINER: &str = "io.nanocl.cargo.container";
pub(crate) const LABEL_ROLE: &str = "io.nanocl.cargo.role";
pub(crate) const LABEL_ESSENTIAL: &str = "io.nanocl.cargo.essential";

/// Role of a declared container in a Cargo replica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CargoContainerRole {
  Application,
  Init,
}

impl CargoContainerRole {
  fn label_value(self) -> &'static str {
    match self {
      Self::Application => "app",
      Self::Init => "init",
    }
  }
}

/// Nanocl-owned identity and environment values supplied by reconciliation.
///
/// The replica identifier is the durable identity. The ordinal is retained as
/// display metadata and for the existing `NANOCL_CARGO_INSTANCE` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CargoRuntimeMetadata<'a> {
  pub(super) cargo_key: &'a str,
  pub(super) namespace: &'a str,
  pub(super) replica_id: &'a str,
  pub(super) replica_ordinal: u32,
  pub(super) node_name: &'a str,
  pub(super) node_address: &'a str,
}

/// Inputs needed to compile one declared application or init container.
#[derive(Debug, Clone, Copy)]
pub(super) struct ContainerCompileInput<'a> {
  pub(super) declared: &'a ContainerSpec,
  pub(super) cargo_network_mode: Option<&'a CargoNetworkMode>,
  pub(super) sandbox_id: Option<&'a str>,
  pub(super) role: CargoContainerRole,
  pub(super) runtime: CargoRuntimeMetadata<'a>,
  pub(super) internal_gateway: Option<&'a str>,
}

/// Inputs needed to compile the internal network sandbox for one replica.
#[derive(Debug, Clone, Copy)]
pub(super) struct SandboxCompileInput<'a> {
  pub(super) cargo_network_mode: Option<&'a CargoNetworkMode>,
  pub(super) port_bindings: Option<&'a PortMap>,
  pub(super) sandbox_image: &'a str,
  pub(super) runtime: CargoRuntimeMetadata<'a>,
}

/// Effective Docker configuration for one named declared container.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CompiledContainer {
  pub(super) logical_name: String,
  pub(super) role: CargoContainerRole,
  pub(super) config: DockerConfig,
}

/// Effective Docker configuration for a Cargo network sandbox.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CompiledSandbox {
  pub(super) replica_id: String,
  pub(super) config: DockerConfig,
}

/// Pure declared-to-effective Cargo compiler failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CargoCompilerError {
  InvalidContainerSpec {
    container: String,
    source: ContainerSpecValidationError,
  },
  MissingSandboxId {
    container: String,
    cargo_network_mode: String,
  },
  UnexpectedSandboxId {
    container: String,
    cargo_network_mode: String,
  },
  InvalidSandboxId {
    container: String,
    reason: &'static str,
  },
  InvalidNetworkCombination {
    container: String,
    cargo_network_mode: String,
    reason: &'static str,
  },
  InvalidContainerRole {
    container: String,
    role: &'static str,
    reason: &'static str,
  },
  InvalidCargoPortBindingCombination {
    cargo_network_mode: String,
  },
  InvalidCargoPortBinding {
    port_key: String,
    host_port: Option<String>,
    reason: &'static str,
  },
  UnresolvedInternalGateway {
    container: String,
  },
  InternalGatewayMapKeyCollision {
    container: String,
    key: String,
  },
  ReservedLabelCollision {
    container: String,
    label: String,
  },
  ReservedEnvironmentCollision {
    container: String,
    variable: String,
  },
  InvalidSandboxImage,
  InvalidEffectiveConfiguration {
    container: String,
    reason: String,
  },
}

impl std::fmt::Display for CargoCompilerError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::InvalidContainerSpec { container, source } => {
        write!(f, "invalid ContainerSpec {container:?}: {source}")
      }
      Self::MissingSandboxId {
        container,
        cargo_network_mode,
      } => write!(
        f,
        "container {container:?} requires a sandbox Docker ID for Cargo network mode {cargo_network_mode:?}"
      ),
      Self::UnexpectedSandboxId {
        container,
        cargo_network_mode,
      } => write!(
        f,
        "container {container:?} received a sandbox Docker ID for incompatible Cargo network mode {cargo_network_mode:?}"
      ),
      Self::InvalidSandboxId { container, reason } => write!(
        f,
        "container {container:?} received an invalid sandbox Docker ID: {reason}"
      ),
      Self::InvalidNetworkCombination {
        container,
        cargo_network_mode,
        reason,
      } => write!(
        f,
        "container {container:?} has an invalid Cargo network mode {cargo_network_mode:?} combination: {reason}"
      ),
      Self::InvalidContainerRole {
        container,
        role,
        reason,
      } => write!(
        f,
        "container {container:?} is invalid for Cargo role {role:?}: {reason}"
      ),
      Self::InvalidCargoPortBindingCombination { cargo_network_mode } => {
        write!(
          f,
          "Cargo port bindings are invalid with Cargo network mode {cargo_network_mode:?}"
        )
      }
      Self::InvalidCargoPortBinding {
        port_key,
        host_port,
        reason,
      } => {
        write!(f, "invalid Cargo port binding {port_key:?}")?;
        if let Some(host_port) = host_port {
          write!(f, " with HostPort {host_port:?}")?;
        }
        write!(f, ": {reason}")
      }
      Self::UnresolvedInternalGateway { container } => write!(
        f,
        "container {container:?} contains {INTERNAL_GATEWAY_TOKEN} but no resolved internal gateway was supplied"
      ),
      Self::InternalGatewayMapKeyCollision { container, key } => write!(
        f,
        "container {container:?} internal gateway replacement creates duplicate object key {key:?}"
      ),
      Self::ReservedLabelCollision { container, label } => write!(
        f,
        "container {container:?} declares reserved Nanocl label {label:?}"
      ),
      Self::ReservedEnvironmentCollision {
        container,
        variable,
      } => write!(
        f,
        "container {container:?} declares reserved Nanocl environment variable {variable:?}"
      ),
      Self::InvalidSandboxImage => {
        f.write_str("Cargo sandbox image cannot be empty")
      }
      Self::InvalidEffectiveConfiguration { container, reason } => write!(
        f,
        "unable to compile the effective Docker configuration for container {container:?}: {reason}"
      ),
    }
  }
}

impl std::error::Error for CargoCompilerError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::InvalidContainerSpec { source, .. } => Some(source),
      _ => None,
    }
  }
}

impl From<CargoCompilerError> for IoError {
  fn from(error: CargoCompilerError) -> Self {
    let message = error.to_string();
    match error {
      CargoCompilerError::InvalidContainerSpec { .. }
      | CargoCompilerError::InvalidNetworkCombination { .. }
      | CargoCompilerError::InvalidCargoPortBindingCombination { .. }
      | CargoCompilerError::InvalidCargoPortBinding { .. }
      | CargoCompilerError::InternalGatewayMapKeyCollision { .. }
      | CargoCompilerError::ReservedLabelCollision { .. }
      | CargoCompilerError::ReservedEnvironmentCollision { .. } => {
        IoError::invalid_input("CargoCompiler", message.as_str())
      }
      CargoCompilerError::MissingSandboxId { .. }
      | CargoCompilerError::UnexpectedSandboxId { .. }
      | CargoCompilerError::InvalidSandboxId { .. }
      | CargoCompilerError::InvalidContainerRole { .. }
      | CargoCompilerError::UnresolvedInternalGateway { .. }
      | CargoCompilerError::InvalidSandboxImage
      | CargoCompilerError::InvalidEffectiveConfiguration { .. } => {
        IoError::other("CargoCompiler", message.as_str())
      }
    }
  }
}

/// Compile a declared container into a separate effective Docker config.
pub(super) fn compile_container(
  input: ContainerCompileInput<'_>,
) -> Result<CompiledContainer, CargoCompilerError> {
  let name = input.declared.name.clone();
  input.declared.validate().map_err(|source| {
    CargoCompilerError::InvalidContainerSpec {
      container: name.clone(),
      source,
    }
  })?;
  if input.role == CargoContainerRole::Init && !input.declared.essential {
    return Err(CargoCompilerError::InvalidContainerRole {
      container: name,
      role: input.role.label_value(),
      reason: "init containers must be essential",
    });
  }

  reject_reserved_values(input.declared)?;

  let cargo_mode = effective_cargo_mode(input.cargo_network_mode);
  if cargo_mode == "host" && input.sandbox_id.is_some() {
    return Err(CargoCompilerError::UnexpectedSandboxId {
      container: name,
      cargo_network_mode: cargo_mode.to_owned(),
    });
  }

  let declared_container_mode = input
    .declared
    .container_config
    .host_config
    .as_ref()
    .and_then(|host_config| host_config.network_mode.as_deref());
  let effective_container_mode = match declared_container_mode {
    Some(mode @ ("host" | "none")) => mode.to_owned(),
    Some(_) => {
      return Err(CargoCompilerError::InvalidNetworkCombination {
        container: name,
        cargo_network_mode: cargo_mode.to_owned(),
        reason: "declared container network mode must be omitted, host, or none",
      });
    }
    None if cargo_mode == "host" => "host".to_owned(),
    None => {
      let sandbox_id = input.sandbox_id.ok_or_else(|| {
        CargoCompilerError::MissingSandboxId {
          container: name.clone(),
          cargo_network_mode: cargo_mode.to_owned(),
        }
      })?;
      if sandbox_id.is_empty() {
        return Err(CargoCompilerError::InvalidSandboxId {
          container: name,
          reason: "sandbox Docker ID cannot be empty",
        });
      }
      format!("container:{sandbox_id}")
    }
  };
  if effective_container_mode.starts_with("container:") {
    validate_shared_network_fields(
      input.declared,
      cargo_mode,
      &input.declared.name,
    )?;
  }

  let mut config = input.declared.container_config.clone();
  expand_internal_gateway(
    &mut config,
    input.internal_gateway,
    &input.declared.name,
  )?;

  let mut host_config = config.host_config.take().unwrap_or_default();
  host_config.network_mode = Some(effective_container_mode);
  if input.role == CargoContainerRole::Application
    && host_config.restart_policy.is_none()
  {
    host_config.restart_policy = Some(RestartPolicy {
      name: Some(RestartPolicyNameEnum::ALWAYS),
      maximum_retry_count: None,
    });
  }
  config.host_config = Some(host_config);

  let mut labels = config.labels.take().unwrap_or_default();
  labels.extend(identity_labels(
    input.runtime,
    input.role.label_value(),
    &input.declared.name,
    input.declared.essential,
  ));
  labels.insert(
    LABEL_COMPOSE_PROJECT.to_owned(),
    format!("nanocl_{}", input.runtime.namespace),
  );
  match input.role {
    CargoContainerRole::Application => {
      labels.insert(LABEL_APPLICATION_CONTAINER.to_owned(), "true".to_owned());
    }
    CargoContainerRole::Init => {
      labels.insert(LABEL_INIT_CONTAINER.to_owned(), "true".to_owned());
    }
  }
  config.labels = Some(labels);

  let mut env = config.env.take().unwrap_or_default();
  env.extend(runtime_environment(input.runtime));
  config.env = Some(env);

  Ok(CompiledContainer {
    logical_name: input.declared.name.clone(),
    role: input.role,
    config,
  })
}

/// Compile the internal network sandbox for one Cargo replica.
///
/// Cargo-level `host` has no sandbox and therefore returns `Ok(None)`.
pub(super) fn compile_sandbox(
  input: SandboxCompileInput<'_>,
) -> Result<Option<CompiledSandbox>, CargoCompilerError> {
  let cargo_mode = effective_cargo_mode(input.cargo_network_mode);
  let has_ports = input.port_bindings.is_some_and(|ports| !ports.is_empty());
  if matches!(cargo_mode, "host" | "none") && has_ports {
    return Err(CargoCompilerError::InvalidCargoPortBindingCombination {
      cargo_network_mode: cargo_mode.to_owned(),
    });
  }
  if cargo_mode == "host" {
    return Ok(None);
  }
  if input.sandbox_image.trim().is_empty() {
    return Err(CargoCompilerError::InvalidSandboxImage);
  }
  if let Some(port_bindings) = input.port_bindings {
    validate_port_bindings(port_bindings)?;
  }

  let port_bindings = input.port_bindings.cloned();
  let exposed_ports = input.port_bindings.map(|ports| {
    ports
      .keys()
      .cloned()
      .map(|port| (port, EmptyObject::default()))
      .collect::<HashMap<_, _>>()
  });
  let labels =
    identity_labels(input.runtime, "sandbox", SANDBOX_LOGICAL_NAME, true);
  let config = DockerConfig {
    image: Some(input.sandbox_image.to_owned()),
    exposed_ports,
    labels: Some(labels),
    host_config: Some(HostConfig {
      network_mode: Some(cargo_mode.to_owned()),
      port_bindings,
      restart_policy: Some(RestartPolicy {
        name: Some(RestartPolicyNameEnum::ALWAYS),
        maximum_retry_count: None,
      }),
      ..Default::default()
    }),
    ..Default::default()
  };

  Ok(Some(CompiledSandbox {
    replica_id: input.runtime.replica_id.to_owned(),
    config,
  }))
}

fn effective_cargo_mode(mode: Option<&CargoNetworkMode>) -> &str {
  mode.map_or(DEFAULT_NETWORK, CargoNetworkMode::as_str)
}

fn identity_labels(
  runtime: CargoRuntimeMetadata<'_>,
  role: &str,
  logical_name: &str,
  essential: bool,
) -> HashMap<String, String> {
  HashMap::from([
    (LABEL_MANAGED.to_owned(), "enabled".to_owned()),
    (LABEL_KIND.to_owned(), "cargo".to_owned()),
    (LABEL_CARGO_KEY.to_owned(), runtime.cargo_key.to_owned()),
    (LABEL_NAMESPACE.to_owned(), runtime.namespace.to_owned()),
    (LABEL_REPLICA.to_owned(), runtime.replica_id.to_owned()),
    (
      LABEL_REPLICA_ORDINAL.to_owned(),
      runtime.replica_ordinal.to_string(),
    ),
    (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
    (LABEL_ROLE.to_owned(), role.to_owned()),
    (LABEL_ESSENTIAL.to_owned(), essential.to_string()),
  ])
}

fn runtime_environment(runtime: CargoRuntimeMetadata<'_>) -> [String; 5] {
  [
    format!("NANOCL_NODE={}", runtime.node_name),
    format!("NANOCL_NODE_ADDR={}", runtime.node_address),
    format!("NANOCL_CARGO_KEY={}", runtime.cargo_key),
    format!("NANOCL_CARGO_NAMESPACE={}", runtime.namespace),
    format!("NANOCL_CARGO_INSTANCE={}", runtime.replica_ordinal),
  ]
}

fn validate_shared_network_fields(
  declared: &ContainerSpec,
  cargo_network_mode: &str,
  container: &str,
) -> Result<(), CargoCompilerError> {
  let config = &declared.container_config;
  let host_config = config.host_config.as_ref();
  let reason = [
    (
      config.hostname.is_some(),
      "Container.Hostname is incompatible with a shared sandbox network namespace",
    ),
    (
      config.mac_address.is_some(),
      "Container.MacAddress is incompatible with a shared sandbox network namespace",
    ),
    (
      config.exposed_ports.is_some(),
      "Container.ExposedPorts is incompatible with a shared sandbox network namespace",
    ),
    (
      host_config.is_some_and(|host| host.dns.is_some()),
      "Container.HostConfig.Dns is incompatible with a shared sandbox network namespace",
    ),
    (
      host_config.is_some_and(|host| host.dns_options.is_some()),
      "Container.HostConfig.DnsOptions is incompatible with a shared sandbox network namespace",
    ),
    (
      host_config.is_some_and(|host| host.dns_search.is_some()),
      "Container.HostConfig.DnsSearch is incompatible with a shared sandbox network namespace",
    ),
    (
      host_config.is_some_and(|host| host.extra_hosts.is_some()),
      "Container.HostConfig.ExtraHosts is incompatible with a shared sandbox network namespace",
    ),
    (
      host_config.is_some_and(|host| host.links.is_some()),
      "Container.HostConfig.Links is incompatible with a shared sandbox network namespace",
    ),
  ]
  .into_iter()
  .find_map(|(present, reason)| present.then_some(reason));

  if let Some(reason) = reason {
    return Err(CargoCompilerError::InvalidNetworkCombination {
      container: container.to_owned(),
      cargo_network_mode: cargo_network_mode.to_owned(),
      reason,
    });
  }
  Ok(())
}

fn validate_port_bindings(
  port_bindings: &PortMap,
) -> Result<(), CargoCompilerError> {
  for (port_key, bindings) in port_bindings {
    let Some((container_port, protocol)) = port_key.split_once('/') else {
      return Err(invalid_port_binding(
        port_key,
        None,
        "key must use <container-port>/<tcp|udp|sctp>",
      ));
    };
    if container_port
      .parse::<u16>()
      .ok()
      .filter(|port| *port != 0)
      .is_none()
    {
      return Err(invalid_port_binding(
        port_key,
        None,
        "container port must be an integer from 1 through 65535",
      ));
    }
    if !matches!(protocol, "tcp" | "udp" | "sctp") {
      return Err(invalid_port_binding(
        port_key,
        None,
        "protocol must be tcp, udp, or sctp",
      ));
    }

    if let Some(bindings) = bindings {
      for binding in bindings {
        let Some(host_port) = binding.host_port.as_deref() else {
          continue;
        };
        if !valid_host_port(host_port) {
          return Err(invalid_port_binding(
            port_key,
            Some(host_port),
            "HostPort must be empty, 0, an integer from 1 through 65535, or an ascending range",
          ));
        }
      }
    }
  }
  Ok(())
}

fn invalid_port_binding(
  port_key: &str,
  host_port: Option<&str>,
  reason: &'static str,
) -> CargoCompilerError {
  CargoCompilerError::InvalidCargoPortBinding {
    port_key: port_key.to_owned(),
    host_port: host_port.map(str::to_owned),
    reason,
  }
}

fn valid_host_port(host_port: &str) -> bool {
  if host_port.is_empty() || host_port == "0" {
    return true;
  }
  if let Some((start, end)) = host_port.split_once('-') {
    if end.contains('-') {
      return false;
    }
    let (Ok(start), Ok(end)) = (start.parse::<u16>(), end.parse::<u16>())
    else {
      return false;
    };
    return start != 0 && start <= end;
  }
  host_port.parse::<u16>().is_ok_and(|port| port != 0)
}

fn reject_reserved_values(
  declared: &ContainerSpec,
) -> Result<(), CargoCompilerError> {
  if let Some(labels) = declared.container_config.labels.as_ref()
    && let Some(label) =
      labels.keys().filter(|label| is_reserved_label(label)).min()
  {
    return Err(CargoCompilerError::ReservedLabelCollision {
      container: declared.name.clone(),
      label: label.clone(),
    });
  }

  if let Some(env) = declared.container_config.env.as_ref() {
    for value in env {
      let variable =
        value.split_once('=').map_or(value.as_str(), |(key, _)| key);
      if variable.starts_with("NANOCL_") {
        return Err(CargoCompilerError::ReservedEnvironmentCollision {
          container: declared.name.clone(),
          variable: variable.to_owned(),
        });
      }
    }
  }

  Ok(())
}

fn is_reserved_label(label: &str) -> bool {
  label == LABEL_MANAGED
    || label.starts_with("io.nanocl.")
    || label == LABEL_COMPOSE_PROJECT
}

fn expand_internal_gateway(
  config: &mut DockerConfig,
  internal_gateway: Option<&str>,
  container: &str,
) -> Result<(), CargoCompilerError> {
  let mut value = serde_json::to_value(&*config).map_err(|error| {
    CargoCompilerError::InvalidEffectiveConfiguration {
      container: container.to_owned(),
      reason: error.to_string(),
    }
  })?;

  if !json_contains_token(&value) {
    return Ok(());
  }
  let gateway = internal_gateway.ok_or_else(|| {
    CargoCompilerError::UnresolvedInternalGateway {
      container: container.to_owned(),
    }
  })?;
  replace_token_in_json(&mut value, gateway).map_err(|key| {
    CargoCompilerError::InternalGatewayMapKeyCollision {
      container: container.to_owned(),
      key,
    }
  })?;
  if json_contains_token(&value) {
    return Err(CargoCompilerError::UnresolvedInternalGateway {
      container: container.to_owned(),
    });
  }

  *config = serde_json::from_value(value).map_err(|error| {
    CargoCompilerError::InvalidEffectiveConfiguration {
      container: container.to_owned(),
      reason: error.to_string(),
    }
  })?;
  Ok(())
}

fn json_contains_token(value: &serde_json::Value) -> bool {
  match value {
    serde_json::Value::String(value) => value.contains(INTERNAL_GATEWAY_TOKEN),
    serde_json::Value::Array(values) => values.iter().any(json_contains_token),
    serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
      key.contains(INTERNAL_GATEWAY_TOKEN) || json_contains_token(value)
    }),
    _ => false,
  }
}

fn replace_token_in_json(
  value: &mut serde_json::Value,
  gateway: &str,
) -> Result<(), String> {
  match value {
    serde_json::Value::String(value) => {
      *value = value.replace(INTERNAL_GATEWAY_TOKEN, gateway);
    }
    serde_json::Value::Array(values) => {
      for value in values {
        replace_token_in_json(value, gateway)?;
      }
    }
    serde_json::Value::Object(values) => {
      let original = std::mem::take(values);
      for (key, mut value) in original {
        replace_token_in_json(&mut value, gateway)?;
        let key = key.replace(INTERNAL_GATEWAY_TOKEN, gateway);
        if values.insert(key.clone(), value).is_some() {
          return Err(key);
        }
      }
    }
    _ => {}
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use bollard_next::models::{
    DeviceMapping, HealthConfig, Mount, MountTypeEnum, PortBinding,
  };
  use nanocl_stubs::generic::ImagePullPolicy;

  use super::*;

  const SANDBOX_ID: &str = "sandbox-docker-id";
  const GATEWAY: &str = "172.18.0.1";

  fn runtime() -> CargoRuntimeMetadata<'static> {
    CargoRuntimeMetadata {
      cargo_key: "demo.web",
      namespace: "demo",
      replica_id: "7a9778c5-6e52-49a0-99f8-b63bb15a4337",
      replica_ordinal: 2,
      node_name: "node-a",
      node_address: "10.0.0.12",
    }
  }

  fn minimal_spec() -> ContainerSpec {
    ContainerSpec {
      name: "app".to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::IfNotPresent,
      container_config: DockerConfig {
        image: Some("example/app:1".to_owned()),
        ..Default::default()
      },
    }
  }

  fn compile(
    declared: &ContainerSpec,
    cargo_network_mode: Option<&CargoNetworkMode>,
    sandbox_id: Option<&str>,
    role: CargoContainerRole,
    internal_gateway: Option<&str>,
  ) -> Result<CompiledContainer, CargoCompilerError> {
    compile_container(ContainerCompileInput {
      declared,
      cargo_network_mode,
      sandbox_id,
      role,
      runtime: runtime(),
      internal_gateway,
    })
  }

  fn compile_application(
    declared: &ContainerSpec,
    cargo_network_mode: Option<&CargoNetworkMode>,
    sandbox_id: Option<&str>,
  ) -> Result<CompiledContainer, CargoCompilerError> {
    compile(
      declared,
      cargo_network_mode,
      sandbox_id,
      CargoContainerRole::Application,
      None,
    )
  }

  fn sandbox(
    cargo_network_mode: Option<&CargoNetworkMode>,
    port_bindings: Option<&PortMap>,
  ) -> Result<Option<CompiledSandbox>, CargoCompilerError> {
    compile_sandbox(SandboxCompileInput {
      cargo_network_mode,
      port_bindings,
      sandbox_image: "registry.nanocl.io/pause@sha256:abc",
      runtime: runtime(),
    })
  }

  fn network_mode(config: &DockerConfig) -> Option<&str> {
    config
      .host_config
      .as_ref()
      .and_then(|host| host.network_mode.as_deref())
  }

  fn assert_io_conversion(
    compiler_error: CargoCompilerError,
    expected_kind: std::io::ErrorKind,
  ) {
    let expected_message = compiler_error.to_string();
    let io_error: IoError = compiler_error.into();

    assert_eq!(io_error.inner.kind(), expected_kind);
    assert_eq!(io_error.context(), Some("CargoCompiler"));
    assert_eq!(io_error.inner.to_string(), expected_message);
  }

  #[test]
  fn compiler_entry_points_keep_typed_cargo_compiler_errors() {
    let _: for<'a> fn(
      ContainerCompileInput<'a>,
    ) -> Result<CompiledContainer, CargoCompilerError> = compile_container;
    let _: for<'a> fn(
      SandboxCompileInput<'a>,
    )
      -> Result<Option<CompiledSandbox>, CargoCompilerError> = compile_sandbox;
  }

  #[test]
  fn compilation_does_not_mutate_the_declared_container_spec() {
    let mut declared = minimal_spec();
    declared.container_config.env =
      Some(vec![format!("PROXY=http://{INTERNAL_GATEWAY_TOKEN}:8080")]);
    let original = declared.clone();

    let compiled = compile(
      &declared,
      None,
      Some(SANDBOX_ID),
      CargoContainerRole::Application,
      Some(GATEWAY),
    )
    .unwrap();

    assert_eq!(declared, original);
    assert_eq!(
      declared.container_config.env.as_ref().unwrap()[0],
      "PROXY=http://$$INTERNAL_GATEWAY:8080"
    );
    assert_eq!(
      compiled.config.env.as_ref().unwrap()[0],
      "PROXY=http://172.18.0.1:8080"
    );
  }

  #[test]
  fn unrelated_bollard_configuration_survives_compilation() {
    let mut declared = minimal_spec();
    declared.container_config.cmd =
      Some(vec!["serve".to_owned(), "--http".to_owned()]);
    declared.container_config.entrypoint = Some(vec!["/entrypoint".to_owned()]);
    declared.container_config.env = Some(vec!["USER_VALUE=present".to_owned()]);
    declared.container_config.working_dir = Some("/srv/app".to_owned());
    declared.container_config.hostname = Some("authored-hostname".to_owned());
    declared.container_config.attach_stdout = Some(false);
    declared.container_config.attach_stderr = Some(false);
    declared.container_config.tty = Some(false);
    declared.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec!["CMD".to_owned(), "check".to_owned()]),
      interval: Some(1_000_000_000),
      retries: Some(3),
      ..Default::default()
    });
    declared.container_config.exposed_ports = Some(HashMap::from([(
      "9000/tcp".to_owned(),
      EmptyObject::default(),
    )]));
    declared.container_config.labels = Some(HashMap::from([(
      "example.owner".to_owned(),
      "user".to_owned(),
    )]));
    let explicit_restart = RestartPolicy {
      name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
      maximum_retry_count: None,
    };
    declared.container_config.host_config = Some(HostConfig {
      binds: Some(vec!["/srv/config:/config:ro".to_owned()]),
      mounts: Some(vec![Mount {
        target: Some("/data".to_owned()),
        source: Some("app-data".to_owned()),
        typ: Some(MountTypeEnum::VOLUME),
        read_only: Some(false),
        ..Default::default()
      }]),
      devices: Some(vec![DeviceMapping {
        path_on_host: Some("/dev/fuse".to_owned()),
        path_in_container: Some("/dev/fuse".to_owned()),
        cgroup_permissions: Some("rwm".to_owned()),
      }]),
      cap_add: Some(vec!["SYS_ADMIN".to_owned()]),
      memory: Some(128 * 1024 * 1024),
      nano_cpus: Some(500_000_000),
      restart_policy: Some(explicit_restart.clone()),
      network_mode: Some("host".to_owned()),
      ..Default::default()
    });

    let compiled = compile_application(&declared, None, None).unwrap().config;
    let host = compiled.host_config.as_ref().unwrap();

    assert_eq!(compiled.image, declared.container_config.image);
    assert_eq!(compiled.cmd, declared.container_config.cmd);
    assert_eq!(compiled.entrypoint, declared.container_config.entrypoint);
    assert_eq!(compiled.working_dir, declared.container_config.working_dir);
    assert_eq!(compiled.hostname, declared.container_config.hostname);
    assert_eq!(compiled.healthcheck, declared.container_config.healthcheck);
    assert_eq!(
      compiled.exposed_ports,
      declared.container_config.exposed_ports
    );
    assert_eq!(compiled.attach_stdout, Some(false));
    assert_eq!(compiled.attach_stderr, Some(false));
    assert_eq!(compiled.tty, Some(false));
    assert_eq!(
      host.binds,
      declared
        .container_config
        .host_config
        .as_ref()
        .unwrap()
        .binds
    );
    assert_eq!(
      host.mounts,
      declared
        .container_config
        .host_config
        .as_ref()
        .unwrap()
        .mounts
    );
    assert_eq!(
      host.devices,
      declared
        .container_config
        .host_config
        .as_ref()
        .unwrap()
        .devices
    );
    assert_eq!(host.cap_add, Some(vec!["SYS_ADMIN".to_owned()]));
    assert_eq!(host.memory, Some(128 * 1024 * 1024));
    assert_eq!(host.nano_cpus, Some(500_000_000));
    assert_eq!(host.restart_policy, Some(explicit_restart));
    assert_eq!(compiled.labels.as_ref().unwrap()["example.owner"], "user");
    assert_eq!(compiled.env.as_ref().unwrap()[0], "USER_VALUE=present");
  }

  #[test]
  fn sandbox_backed_cargo_modes_compile_to_the_sandbox_namespace() {
    for cargo_mode in [
      None,
      Some("nanoclbr0"),
      Some("private-api"),
      Some("bridge"),
      Some("default"),
      Some("none"),
    ] {
      let cargo_mode =
        cargo_mode.map(|mode| CargoNetworkMode::new(mode).unwrap());
      let compiled = compile_application(
        &minimal_spec(),
        cargo_mode.as_ref(),
        Some(SANDBOX_ID),
      )
      .unwrap();
      assert_eq!(
        network_mode(&compiled.config),
        Some("container:sandbox-docker-id")
      );
    }
  }

  #[test]
  fn missing_sandbox_id_fails_for_a_normal_sandbox_backed_container() {
    assert_eq!(
      compile_application(&minimal_spec(), None, None).unwrap_err(),
      CargoCompilerError::MissingSandboxId {
        container: "app".to_owned(),
        cargo_network_mode: "nanoclbr0".to_owned(),
      }
    );
  }

  #[test]
  fn empty_sandbox_id_is_an_internal_context_error() {
    let error =
      compile_application(&minimal_spec(), None, Some("")).unwrap_err();
    assert_eq!(
      error,
      CargoCompilerError::InvalidSandboxId {
        container: "app".to_owned(),
        reason: "sandbox Docker ID cannot be empty",
      }
    );
    assert_io_conversion(error, std::io::ErrorKind::Other);
  }

  #[test]
  fn cargo_host_compiles_an_omitted_container_mode_to_host() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    let compiled =
      compile_application(&minimal_spec(), Some(&cargo_mode), None).unwrap();
    assert_eq!(network_mode(&compiled.config), Some("host"));
  }

  #[test]
  fn cargo_host_preserves_an_explicit_container_none_override() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    let mut declared = minimal_spec();
    declared.container_config.host_config = Some(HostConfig {
      network_mode: Some("none".to_owned()),
      ..Default::default()
    });

    let compiled =
      compile_application(&declared, Some(&cargo_mode), None).unwrap();
    assert_eq!(network_mode(&compiled.config), Some("none"));
  }

  #[test]
  fn cargo_host_rejects_a_supplied_sandbox_id() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    assert_eq!(
      compile_application(
        &minimal_spec(),
        Some(&cargo_mode),
        Some(SANDBOX_ID),
      )
      .unwrap_err(),
      CargoCompilerError::UnexpectedSandboxId {
        container: "app".to_owned(),
        cargo_network_mode: "host".to_owned(),
      }
    );
  }

  #[test]
  fn explicit_container_host_and_none_bypass_the_cargo_sandbox() {
    let exposed_ports =
      HashMap::from([("8080/tcp".to_owned(), EmptyObject::default())]);
    for override_mode in ["host", "none"] {
      let mut declared = minimal_spec();
      declared.container_config.exposed_ports = Some(exposed_ports.clone());
      declared.container_config.host_config = Some(HostConfig {
        network_mode: Some(override_mode.to_owned()),
        ..Default::default()
      });

      let compiled = compile_application(&declared, None, None).unwrap();
      assert_eq!(network_mode(&compiled.config), Some(override_mode));
      assert_eq!(compiled.config.exposed_ports, Some(exposed_ports.clone()));
    }
  }

  #[test]
  fn invalid_nested_network_mode_cannot_be_compiled_without_validation() {
    let mut declared = minimal_spec();
    declared.container_config.host_config = Some(HostConfig {
      network_mode: Some("bridge".to_owned()),
      ..Default::default()
    });

    assert_eq!(
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err(),
      CargoCompilerError::InvalidContainerSpec {
        container: "app".to_owned(),
        source: ContainerSpecValidationError::InvalidNetworkMode,
      }
    );
  }

  #[test]
  fn sandbox_join_rejects_container_owned_network_settings() {
    for (field, reason) in [
      (
        "Hostname",
        "Container.Hostname is incompatible with a shared sandbox network namespace",
      ),
      (
        "MacAddress",
        "Container.MacAddress is incompatible with a shared sandbox network namespace",
      ),
      (
        "ExposedPorts",
        "Container.ExposedPorts is incompatible with a shared sandbox network namespace",
      ),
      (
        "Dns",
        "Container.HostConfig.Dns is incompatible with a shared sandbox network namespace",
      ),
      (
        "DnsOptions",
        "Container.HostConfig.DnsOptions is incompatible with a shared sandbox network namespace",
      ),
      (
        "DnsSearch",
        "Container.HostConfig.DnsSearch is incompatible with a shared sandbox network namespace",
      ),
      (
        "ExtraHosts",
        "Container.HostConfig.ExtraHosts is incompatible with a shared sandbox network namespace",
      ),
      (
        "Links",
        "Container.HostConfig.Links is incompatible with a shared sandbox network namespace",
      ),
    ] {
      let mut declared = minimal_spec();
      match field {
        "Hostname" => {
          declared.container_config.hostname = Some("app".to_owned())
        }
        "MacAddress" => {
          declared.container_config.mac_address =
            Some("02:42:ac:11:00:02".to_owned());
        }
        "ExposedPorts" => {
          declared.container_config.exposed_ports = Some(HashMap::from([(
            "8080/tcp".to_owned(),
            EmptyObject::default(),
          )]));
        }
        _ => {
          let host = declared
            .container_config
            .host_config
            .get_or_insert_default();
          match field {
            "Dns" => host.dns = Some(vec!["1.1.1.1".to_owned()]),
            "DnsOptions" => host.dns_options = Some(vec!["ndots:1".to_owned()]),
            "DnsSearch" => {
              host.dns_search = Some(vec!["example.test".to_owned()])
            }
            "ExtraHosts" => {
              host.extra_hosts = Some(vec!["api:127.0.0.1".to_owned()])
            }
            "Links" => host.links = Some(vec!["database:db".to_owned()]),
            _ => unreachable!(),
          }
        }
      }

      for role in [CargoContainerRole::Application, CargoContainerRole::Init] {
        let error =
          compile(&declared, None, Some(SANDBOX_ID), role, None).unwrap_err();
        assert!(
          error.to_string().contains(field),
          "inherited-network diagnostic must identify {field}: {error}"
        );
        assert_eq!(
          error,
          CargoCompilerError::InvalidNetworkCombination {
            container: "app".to_owned(),
            cargo_network_mode: "nanoclbr0".to_owned(),
            reason,
          },
          "unexpected inherited-network validation for {field} on {role:?}"
        );
      }
    }
  }

  #[test]
  fn sandbox_join_preserves_unrelated_bollard_configuration() {
    let mut declared = minimal_spec();
    declared.container_config.cmd =
      Some(vec!["serve".to_owned(), "--http".to_owned()]);
    declared.container_config.working_dir = Some("/srv/app".to_owned());
    declared.container_config.host_config = Some(HostConfig {
      binds: Some(vec!["/srv/config:/config:ro".to_owned()]),
      ..Default::default()
    });

    let compiled =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap();
    assert_eq!(compiled.config.cmd, declared.container_config.cmd);
    assert_eq!(
      compiled.config.working_dir,
      declared.container_config.working_dir
    );
    assert_eq!(
      compiled.config.host_config.unwrap().binds,
      declared.container_config.host_config.unwrap().binds
    );
  }

  #[test]
  fn cargo_host_preserves_container_owned_network_settings() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    let mut declared = minimal_spec();
    declared.container_config.hostname = Some("app".to_owned());
    declared.container_config.mac_address =
      Some("02:42:ac:11:00:02".to_owned());
    declared.container_config.host_config = Some(HostConfig {
      dns: Some(vec!["1.1.1.1".to_owned()]),
      dns_options: Some(vec!["ndots:1".to_owned()]),
      dns_search: Some(vec!["example.test".to_owned()]),
      extra_hosts: Some(vec!["api:127.0.0.1".to_owned()]),
      ..Default::default()
    });

    let compiled = compile_application(&declared, Some(&cargo_mode), None)
      .unwrap()
      .config;
    assert_eq!(compiled.hostname, declared.container_config.hostname);
    assert_eq!(compiled.mac_address, declared.container_config.mac_address);
    let host = compiled.host_config.unwrap();
    let declared_host = declared.container_config.host_config.unwrap();
    assert_eq!(host.dns, declared_host.dns);
    assert_eq!(host.dns_options, declared_host.dns_options);
    assert_eq!(host.dns_search, declared_host.dns_search);
    assert_eq!(host.extra_hosts, declared_host.extra_hosts);
  }

  #[test]
  fn nonessential_init_container_cannot_be_compiled() {
    let mut declared = minimal_spec();
    declared.essential = false;
    assert_eq!(
      compile(
        &declared,
        None,
        Some(SANDBOX_ID),
        CargoContainerRole::Init,
        None,
      )
      .unwrap_err(),
      CargoCompilerError::InvalidContainerRole {
        container: "app".to_owned(),
        role: "init",
        reason: "init containers must be essential",
      }
    );
  }

  fn real_port_map() -> PortMap {
    PortMap::from([
      (
        "8080/tcp".to_owned(),
        Some(vec![
          PortBinding {
            host_ip: Some("127.0.0.1".to_owned()),
            host_port: Some("8080".to_owned()),
          },
          PortBinding {
            host_ip: Some("::1".to_owned()),
            host_port: Some("18080".to_owned()),
          },
        ]),
      ),
      ("8443/tcp".to_owned(), None),
    ])
  }

  #[test]
  fn cargo_port_map_and_exposed_ports_belong_to_the_sandbox() {
    let ports = real_port_map();
    let compiled = sandbox(None, Some(&ports)).unwrap().unwrap();
    let config = compiled.config;

    assert_eq!(
      config.host_config.as_ref().unwrap().port_bindings,
      Some(ports.clone())
    );
    assert_eq!(
      config
        .host_config
        .as_ref()
        .unwrap()
        .restart_policy
        .as_ref()
        .and_then(|policy| policy.name),
      Some(RestartPolicyNameEnum::ALWAYS)
    );
    let exposed = config.exposed_ports.unwrap();
    assert_eq!(exposed.len(), 2);
    assert!(exposed.contains_key("8080/tcp"));
    assert!(exposed.contains_key("8443/tcp"));
    assert_eq!(
      config.host_config.unwrap().port_bindings.unwrap()["8080/tcp"]
        .as_ref()
        .unwrap(),
      ports["8080/tcp"].as_ref().unwrap()
    );
  }

  #[test]
  fn application_and_init_configs_never_receive_port_publication() {
    for role in [CargoContainerRole::Application, CargoContainerRole::Init] {
      let compiled =
        compile(&minimal_spec(), None, Some(SANDBOX_ID), role, None).unwrap();
      let host = compiled.config.host_config.unwrap();
      assert!(host.port_bindings.is_none());
      assert!(host.publish_all_ports.is_none());
    }
  }

  #[test]
  fn cargo_ports_are_rejected_with_host_and_none_modes() {
    let ports = real_port_map();
    for mode in ["host", "none"] {
      let cargo_mode = CargoNetworkMode::new(mode).unwrap();
      assert_eq!(
        sandbox(Some(&cargo_mode), Some(&ports)).unwrap_err(),
        CargoCompilerError::InvalidCargoPortBindingCombination {
          cargo_network_mode: mode.to_owned(),
        }
      );
    }
  }

  #[test]
  fn malformed_cargo_port_keys_are_rejected_before_sandbox_compilation() {
    for port_key in ["8080", "0/tcp", "65536/tcp", "8080/http", "80/tcp/udp"] {
      let ports = PortMap::from([(port_key.to_owned(), None)]);
      assert!(matches!(
        sandbox(None, Some(&ports)).unwrap_err(),
        CargoCompilerError::InvalidCargoPortBinding {
          port_key: failed_key,
          host_port: None,
          ..
        } if failed_key == port_key
      ));
    }
  }

  #[test]
  fn malformed_host_ports_are_rejected_before_sandbox_compilation() {
    for host_port in ["invalid", "65536", "0-80", "9000-8000", "80-90-100"] {
      let ports = PortMap::from([(
        "8080/tcp".to_owned(),
        Some(vec![PortBinding {
          host_ip: None,
          host_port: Some(host_port.to_owned()),
        }]),
      )]);
      assert!(matches!(
        sandbox(None, Some(&ports)).unwrap_err(),
        CargoCompilerError::InvalidCargoPortBinding {
          port_key,
          host_port: Some(failed_port),
          ..
        } if port_key == "8080/tcp" && failed_port == host_port
      ));
    }
  }

  #[test]
  fn dynamic_and_ranged_host_ports_remain_valid_real_bollard_values() {
    let bindings = [None, Some(""), Some("0"), Some("8000"), Some("8000-8002")]
      .into_iter()
      .map(|host_port| PortBinding {
        host_ip: None,
        host_port: host_port.map(str::to_owned),
      })
      .collect::<Vec<_>>();
    let ports = PortMap::from([("8080/tcp".to_owned(), Some(bindings))]);

    let compiled = sandbox(None, Some(&ports)).unwrap().unwrap();
    assert_eq!(
      compiled.config.host_config.unwrap().port_bindings,
      Some(ports)
    );
  }

  #[test]
  fn sandbox_contains_only_image_network_ports_and_identity_metadata() {
    let ports = real_port_map();
    let compiled = sandbox(None, Some(&ports)).unwrap().unwrap();
    let config = compiled.config;
    let host = config.host_config.as_ref().unwrap();

    assert_eq!(
      config.image.as_deref(),
      Some("registry.nanocl.io/pause@sha256:abc")
    );
    assert_eq!(network_mode(&config), Some("nanoclbr0"));
    assert!(config.env.is_none());
    assert!(config.cmd.is_none());
    assert!(config.entrypoint.is_none());
    assert!(config.healthcheck.is_none());
    assert!(config.volumes.is_none());
    assert!(host.binds.is_none());
    assert!(host.mounts.is_none());
    assert!(host.log_config.is_none());
    let labels = config.labels.unwrap();
    assert_eq!(labels[LABEL_ROLE], "sandbox");
    assert_eq!(labels[LABEL_CONTAINER], SANDBOX_LOGICAL_NAME);
    assert_eq!(labels[LABEL_REPLICA], runtime().replica_id);
    assert!(!labels.contains_key(LABEL_COMPOSE_PROJECT));
    assert!(!labels.contains_key(LABEL_APPLICATION_CONTAINER));
    assert!(!labels.contains_key(LABEL_INIT_CONTAINER));
  }

  #[test]
  fn cargo_host_produces_no_sandbox() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    assert!(sandbox(Some(&cargo_mode), None).unwrap().is_none());
  }

  #[test]
  fn cargo_host_needs_no_sandbox_image() {
    let cargo_mode = CargoNetworkMode::new("host").unwrap();
    let compiled = compile_sandbox(SandboxCompileInput {
      cargo_network_mode: Some(&cargo_mode),
      port_bindings: None,
      sandbox_image: "",
      runtime: runtime(),
    })
    .unwrap();
    assert!(compiled.is_none());
  }

  #[test]
  fn cargo_none_produces_a_none_network_sandbox() {
    let cargo_mode = CargoNetworkMode::new("none").unwrap();
    let compiled = sandbox(Some(&cargo_mode), None).unwrap().unwrap();
    assert_eq!(network_mode(&compiled.config), Some("none"));
  }

  #[test]
  fn custom_and_builtin_sandbox_modes_are_expressed_without_docker_calls() {
    for mode in ["private-api", "bridge", "default"] {
      let cargo_mode = CargoNetworkMode::new(mode).unwrap();
      let compiled = sandbox(Some(&cargo_mode), None).unwrap().unwrap();
      assert_eq!(network_mode(&compiled.config), Some(mode));
    }
  }

  #[test]
  fn gateway_replacement_covers_all_declared_bollard_string_locations() {
    let token = INTERNAL_GATEWAY_TOKEN;
    let mut declared = minimal_spec();
    declared.container_config.env =
      Some(vec![format!("UPSTREAM=http://{token}:80")]);
    declared.container_config.cmd = Some(vec![format!("--gateway={token}")]);
    declared.container_config.entrypoint = Some(vec![format!("/run-{token}")]);
    declared.container_config.working_dir = Some(format!("/work/{token}"));
    declared.container_config.healthcheck = Some(HealthConfig {
      test: Some(vec!["CMD".to_owned(), format!("probe-{token}")]),
      ..Default::default()
    });
    declared.container_config.labels = Some(HashMap::from([(
      format!("example.{token}"),
      format!("value-{token}"),
    )]));
    declared.container_config.host_config = Some(HostConfig {
      binds: Some(vec![format!("/host/{token}:/container/{token}:ro")]),
      mounts: Some(vec![Mount {
        source: Some(format!("volume-{token}")),
        target: Some(format!("/target/{token}")),
        typ: Some(MountTypeEnum::VOLUME),
        ..Default::default()
      }]),
      storage_opt: Some(HashMap::from([(
        format!("option-{token}"),
        format!("setting-{token}"),
      )])),
      ..Default::default()
    });

    let compiled = compile(
      &declared,
      None,
      Some(SANDBOX_ID),
      CargoContainerRole::Application,
      Some(GATEWAY),
    )
    .unwrap()
    .config;
    let serialized = serde_json::to_string(&compiled).unwrap();

    assert!(!serialized.contains(INTERNAL_GATEWAY_TOKEN));
    assert!(serialized.contains(GATEWAY));
    assert_eq!(
      compiled.env.as_ref().unwrap()[0],
      "UPSTREAM=http://172.18.0.1:80"
    );
    assert_eq!(compiled.cmd.as_ref().unwrap()[0], "--gateway=172.18.0.1");
    assert_eq!(compiled.entrypoint.as_ref().unwrap()[0], "/run-172.18.0.1");
    assert_eq!(compiled.working_dir.as_deref(), Some("/work/172.18.0.1"));
    assert_eq!(
      compiled
        .healthcheck
        .as_ref()
        .unwrap()
        .test
        .as_ref()
        .unwrap()[1],
      "probe-172.18.0.1"
    );
    assert_eq!(
      compiled
        .host_config
        .as_ref()
        .unwrap()
        .binds
        .as_ref()
        .unwrap()[0],
      "/host/172.18.0.1:/container/172.18.0.1:ro"
    );
    let host = compiled.host_config.as_ref().unwrap();
    let mount = &host.mounts.as_ref().unwrap()[0];
    assert_eq!(mount.source.as_deref(), Some("volume-172.18.0.1"));
    assert_eq!(mount.target.as_deref(), Some("/target/172.18.0.1"));
    assert_eq!(
      host.storage_opt.as_ref().unwrap()["option-172.18.0.1"],
      "setting-172.18.0.1"
    );
    assert_eq!(
      compiled.labels.as_ref().unwrap()["example.172.18.0.1"],
      "value-172.18.0.1"
    );
  }

  #[test]
  fn gateway_replacement_replaces_multiple_tokens_in_one_value() {
    let mut declared = minimal_spec();
    declared.container_config.cmd = Some(vec![format!(
      "{INTERNAL_GATEWAY_TOKEN}:{INTERNAL_GATEWAY_TOKEN}"
    )]);

    let compiled = compile(
      &declared,
      None,
      Some(SANDBOX_ID),
      CargoContainerRole::Application,
      Some(GATEWAY),
    )
    .unwrap();
    assert_eq!(compiled.config.cmd.unwrap()[0], "172.18.0.1:172.18.0.1");
  }

  #[test]
  fn a_configuration_without_the_gateway_token_needs_no_gateway() {
    compile_application(&minimal_spec(), None, Some(SANDBOX_ID)).unwrap();
  }

  #[test]
  fn unresolved_gateway_token_is_a_typed_error() {
    let mut declared = minimal_spec();
    declared.container_config.env =
      Some(vec![format!("HOST={INTERNAL_GATEWAY_TOKEN}")]);

    assert_eq!(
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err(),
      CargoCompilerError::UnresolvedInternalGateway {
        container: "app".to_owned(),
      }
    );
  }

  #[test]
  fn generated_labels_and_environment_are_deterministic() {
    let compiled =
      compile_application(&minimal_spec(), None, Some(SANDBOX_ID)).unwrap();
    let labels = compiled.config.labels.unwrap();
    assert_eq!(labels[LABEL_MANAGED], "enabled");
    assert_eq!(labels[LABEL_KIND], "cargo");
    assert_eq!(labels[LABEL_CARGO_KEY], "demo.web");
    assert_eq!(labels[LABEL_NAMESPACE], "demo");
    assert_eq!(labels[LABEL_COMPOSE_PROJECT], "nanocl_demo");
    assert_eq!(labels[LABEL_REPLICA], runtime().replica_id);
    assert_eq!(labels[LABEL_REPLICA_ORDINAL], "2");
    assert_eq!(labels[LABEL_CONTAINER], "app");
    assert_eq!(labels[LABEL_ROLE], "app");
    assert_eq!(labels[LABEL_APPLICATION_CONTAINER], "true");
    assert!(!labels.contains_key(LABEL_INIT_CONTAINER));
    assert_eq!(
      compiled.config.env.unwrap(),
      [
        "NANOCL_NODE=node-a",
        "NANOCL_NODE_ADDR=10.0.0.12",
        "NANOCL_CARGO_KEY=demo.web",
        "NANOCL_CARGO_NAMESPACE=demo",
        "NANOCL_CARGO_INSTANCE=2",
      ]
    );
  }

  #[test]
  fn init_metadata_is_role_specific_and_has_no_restart_default() {
    let compiled = compile(
      &minimal_spec(),
      None,
      Some(SANDBOX_ID),
      CargoContainerRole::Init,
      None,
    )
    .unwrap();
    let labels = compiled.config.labels.unwrap();
    assert_eq!(labels[LABEL_ROLE], "init");
    assert_eq!(labels[LABEL_INIT_CONTAINER], "true");
    assert!(!labels.contains_key(LABEL_APPLICATION_CONTAINER));
    assert!(
      compiled
        .config
        .host_config
        .unwrap()
        .restart_policy
        .is_none()
    );
  }

  #[test]
  fn application_restart_policy_defaults_to_always() {
    let compiled =
      compile_application(&minimal_spec(), None, Some(SANDBOX_ID)).unwrap();
    assert_eq!(
      compiled
        .config
        .host_config
        .unwrap()
        .restart_policy
        .unwrap()
        .name,
      Some(RestartPolicyNameEnum::ALWAYS)
    );
  }

  #[test]
  fn reserved_nanocl_label_collisions_fail() {
    for label in [
      LABEL_MANAGED,
      LABEL_KIND,
      LABEL_CARGO_KEY,
      LABEL_ROLE,
      LABEL_COMPOSE_PROJECT,
      "io.nanocl.future-field",
    ] {
      let mut declared = minimal_spec();
      declared.container_config.labels =
        Some(HashMap::from([(label.to_owned(), "user-value".to_owned())]));
      assert_eq!(
        compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err(),
        CargoCompilerError::ReservedLabelCollision {
          container: "app".to_owned(),
          label: label.to_owned(),
        }
      );
    }
  }

  #[test]
  fn reserved_nanocl_environment_collisions_fail() {
    for value in [
      "NANOCL_NODE=user-node",
      "NANOCL_CARGO_KEY",
      "NANOCL_FUTURE_VALUE=reserved",
    ] {
      let mut declared = minimal_spec();
      declared.container_config.env = Some(vec![value.to_owned()]);
      let variable = value.split_once('=').map_or(value, |(key, _)| key);
      assert_eq!(
        compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err(),
        CargoCompilerError::ReservedEnvironmentCollision {
          container: "app".to_owned(),
          variable: variable.to_owned(),
        }
      );
    }
  }

  #[test]
  fn invalid_container_spec_converts_to_invalid_input() {
    let mut declared = minimal_spec();
    declared.container_config.image = None;

    let error =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err();
    assert!(matches!(
      &error,
      CargoCompilerError::InvalidContainerSpec { container, .. }
        if container == "app"
    ));
    assert_io_conversion(error, std::io::ErrorKind::InvalidInput);
  }

  #[test]
  fn invalid_port_binding_converts_to_invalid_input() {
    let ports = PortMap::from([("invalid".to_owned(), None)]);
    let error = sandbox(None, Some(&ports)).unwrap_err();
    assert!(matches!(
      &error,
      CargoCompilerError::InvalidCargoPortBinding { port_key, .. }
        if port_key == "invalid"
    ));
    assert_io_conversion(error, std::io::ErrorKind::InvalidInput);
  }

  #[test]
  fn reserved_declarations_convert_to_invalid_input() {
    let mut declared = minimal_spec();
    declared.container_config.labels = Some(HashMap::from([(
      "io.nanocl.user-value".to_owned(),
      "reserved".to_owned(),
    )]));
    let label_error =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err();
    assert!(matches!(
      &label_error,
      CargoCompilerError::ReservedLabelCollision { .. }
    ));
    assert_io_conversion(label_error, std::io::ErrorKind::InvalidInput);

    let mut declared = minimal_spec();
    declared.container_config.env =
      Some(vec!["NANOCL_USER_VALUE=reserved".to_owned()]);
    let environment_error =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err();
    assert!(matches!(
      &environment_error,
      CargoCompilerError::ReservedEnvironmentCollision { .. }
    ));
    assert_io_conversion(environment_error, std::io::ErrorKind::InvalidInput);
  }

  #[test]
  fn missing_sandbox_context_converts_to_other() {
    let error = compile_application(&minimal_spec(), None, None).unwrap_err();
    assert!(matches!(
      &error,
      CargoCompilerError::MissingSandboxId { .. }
    ));
    assert_io_conversion(error, std::io::ErrorKind::Other);
  }

  #[test]
  fn unresolved_internal_gateway_converts_to_other() {
    let mut declared = minimal_spec();
    declared.container_config.env =
      Some(vec![format!("HOST={INTERNAL_GATEWAY_TOKEN}")]);

    let error =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap_err();
    assert!(matches!(
      &error,
      CargoCompilerError::UnresolvedInternalGateway { .. }
    ));
    assert_io_conversion(error, std::io::ErrorKind::Other);
  }

  #[test]
  fn gateway_map_key_collision_converts_to_invalid_input() {
    let mut declared = minimal_spec();
    declared.container_config.labels = Some(HashMap::from([
      (
        format!("example.{INTERNAL_GATEWAY_TOKEN}"),
        "token-key".to_owned(),
      ),
      (format!("example.{GATEWAY}"), "resolved-key".to_owned()),
    ]));

    let error = compile(
      &declared,
      None,
      Some(SANDBOX_ID),
      CargoContainerRole::Application,
      Some(GATEWAY),
    )
    .unwrap_err();
    assert!(matches!(
      &error,
      CargoCompilerError::InternalGatewayMapKeyCollision { key, .. }
        if key == "example.172.18.0.1"
    ));
    assert_io_conversion(error, std::io::ErrorKind::InvalidInput);
  }

  #[test]
  fn every_error_variant_has_exactly_one_io_mapping() {
    let mappings = [
      (
        CargoCompilerError::InvalidContainerSpec {
          container: "app".to_owned(),
          source: ContainerSpecValidationError::MissingImage,
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::MissingSandboxId {
          container: "app".to_owned(),
          cargo_network_mode: "nanoclbr0".to_owned(),
        },
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::UnexpectedSandboxId {
          container: "app".to_owned(),
          cargo_network_mode: "host".to_owned(),
        },
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::InvalidSandboxId {
          container: "app".to_owned(),
          reason: "sandbox Docker ID cannot be empty",
        },
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::InvalidNetworkCombination {
          container: "app".to_owned(),
          cargo_network_mode: "nanoclbr0".to_owned(),
          reason: "authored network field is incompatible with sharing",
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::InvalidContainerRole {
          container: "init".to_owned(),
          role: "init",
          reason: "init containers must be essential",
        },
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::InvalidCargoPortBindingCombination {
          cargo_network_mode: "host".to_owned(),
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::InvalidCargoPortBinding {
          port_key: "invalid".to_owned(),
          host_port: None,
          reason: "port key must use port/protocol syntax",
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::UnresolvedInternalGateway {
          container: "app".to_owned(),
        },
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::InternalGatewayMapKeyCollision {
          container: "app".to_owned(),
          key: "example.172.18.0.1".to_owned(),
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::ReservedLabelCollision {
          container: "app".to_owned(),
          label: "io.nanocl.user-value".to_owned(),
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::ReservedEnvironmentCollision {
          container: "app".to_owned(),
          variable: "NANOCL_USER_VALUE".to_owned(),
        },
        std::io::ErrorKind::InvalidInput,
      ),
      (
        CargoCompilerError::InvalidSandboxImage,
        std::io::ErrorKind::Other,
      ),
      (
        CargoCompilerError::InvalidEffectiveConfiguration {
          container: "app".to_owned(),
          reason: "typed reconstruction invariant".to_owned(),
        },
        std::io::ErrorKind::Other,
      ),
    ];
    let mut covered_variants = Vec::with_capacity(mappings.len());

    for (error, expected_kind) in mappings {
      let variant = std::mem::discriminant(&error);
      assert!(
        !covered_variants.contains(&variant),
        "duplicate mapping for {error:?}"
      );
      covered_variants.push(variant);
      assert_io_conversion(error, expected_kind);
    }

    assert_eq!(covered_variants.len(), 14);
  }

  #[test]
  fn unrelated_user_labels_and_environment_remain_accepted() {
    let mut declared = minimal_spec();
    declared.container_config.labels = Some(HashMap::from([
      ("example.owner".to_owned(), "platform".to_owned()),
      ("io.example.nanocl".to_owned(), "allowed".to_owned()),
    ]));
    declared.container_config.env = Some(vec![
      "APP_ENV=production".to_owned(),
      "BARE_USER_KEY".to_owned(),
    ]);

    let compiled =
      compile_application(&declared, None, Some(SANDBOX_ID)).unwrap();
    let labels = compiled.config.labels.unwrap();
    assert_eq!(labels["example.owner"], "platform");
    assert_eq!(labels["io.example.nanocl"], "allowed");
    let env = compiled.config.env.unwrap();
    assert_eq!(env[0], "APP_ENV=production");
    assert_eq!(env[1], "BARE_USER_KEY");
  }
}
