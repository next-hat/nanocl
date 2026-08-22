use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bollard_next::service::{
  ContainerInspectResponse, ContainerStateStatusEnum,
};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  cargo_spec::{
    CARGO_CONTAINER_POSITION_LABEL, CargoNetworkMode, CargoSpec,
    Config as DockerConfig, ContainerSpec, PortMap,
  },
  generic::ImagePullPolicy,
  resource_key::ResourceKey,
  system::ObjPsStatusKind,
};

use crate::{
  models::CargoReplicaProcessRole,
  utils::container::{
    cargo_compiler::{
      LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA,
      LABEL_REPLICA_ORDINAL, LABEL_ROLE,
    },
    network::DEFAULT_NETWORK,
  },
};

const SANDBOX_LOGICAL_NAME: &str = "_sandbox";
const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
const SECRET_MOUNT_TARGET: &str = "/opt/nanocl.io/secrets";
const RUNTIME_ENVIRONMENT_NAMES: [&str; 5] = [
  "NANOCL_NODE",
  "NANOCL_NODE_ADDR",
  "NANOCL_CARGO_KEY",
  "NANOCL_CARGO_NAMESPACE",
  "NANOCL_CARGO_INSTANCE",
];

fn bootstrap_error(message: impl AsRef<str>) -> IoError {
  IoError::invalid_data("CargoBootstrap", message.as_ref())
}

fn is_rollout_artifact(name: &str) -> bool {
  let name = name.trim_start_matches('/');
  name.starts_with("tmp-") || name.starts_with("candidate-")
}

/// One managed Docker process observed during startup inventory.
#[derive(Debug, Clone)]
pub(super) struct CargoBootstrapProcess {
  pub(super) process_key: String,
  pub(super) process_name: String,
  pub(super) labels: HashMap<String, String>,
  pub(super) config: DockerConfig,
  pub(super) status: Option<ContainerStateStatusEnum>,
}

impl CargoBootstrapProcess {
  pub(super) fn from_inspect(
    instance: &ContainerInspectResponse,
  ) -> IoResult<Self> {
    let process_key = instance
      .id
      .clone()
      .filter(|id| !id.is_empty())
      .ok_or_else(|| {
        bootstrap_error("managed Cargo container has no Docker ID")
      })?;
    let process_name = instance
      .name
      .as_deref()
      .unwrap_or_default()
      .trim_start_matches('/')
      .to_owned();
    let inspected = instance.config.clone().unwrap_or_default();
    let labels = inspected.labels.clone().unwrap_or_default();
    let mut config: DockerConfig = inspected.into();
    config.host_config.clone_from(&instance.host_config);
    let status = instance.state.as_ref().and_then(|state| state.status);
    Ok(Self {
      process_key,
      process_name,
      labels,
      config,
      status,
    })
  }
}

#[derive(Debug, Clone)]
pub(super) struct ReconstructedCargoProcess {
  pub(super) process_key: String,
  pub(super) replica_key: uuid::Uuid,
  pub(super) replica_ordinal: i32,
  pub(super) role: CargoReplicaProcessRole,
  pub(super) container_name: String,
  pub(super) essential: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ReconstructedCargo {
  pub(super) spec: CargoSpec,
  pub(super) processes: Vec<ReconstructedCargoProcess>,
  pub(super) actual: ObjPsStatusKind,
  pub(super) direct_installer_generation: bool,
}

#[derive(Debug, Clone)]
struct ParsedCargoProcess {
  process_key: String,
  process_name: String,
  replica_key: uuid::Uuid,
  replica_ordinal: i32,
  role: CargoReplicaProcessRole,
  container_name: String,
  declaration_position: Option<usize>,
  essential: bool,
  config: DockerConfig,
  status: Option<ContainerStateStatusEnum>,
}

fn required_label<'a>(
  process: &'a CargoBootstrapProcess,
  label: &str,
) -> IoResult<&'a str> {
  process
    .labels
    .get(label)
    .map(String::as_str)
    .ok_or_else(|| {
      bootstrap_error(format!(
        "Cargo process {:?} is missing required label {label:?}",
        process.process_key
      ))
    })
}

fn parse_process(
  cargo_key: &str,
  process: &CargoBootstrapProcess,
) -> IoResult<ParsedCargoProcess> {
  let observed_cargo_key = required_label(process, LABEL_CARGO_KEY)?;
  if observed_cargo_key != cargo_key {
    return Err(bootstrap_error(format!(
      "Cargo process {:?} identifies Cargo {observed_cargo_key:?}, not {cargo_key:?}",
      process.process_key
    )));
  }
  let replica_key = required_label(process, LABEL_REPLICA)?
    .parse::<uuid::Uuid>()
    .map_err(|error| {
      bootstrap_error(format!(
        "Cargo process {:?} has invalid replica UUID: {error}",
        process.process_key
      ))
    })?;
  let replica_ordinal = required_label(process, LABEL_REPLICA_ORDINAL)?
    .parse::<i32>()
    .map_err(|error| {
      bootstrap_error(format!(
        "Cargo process {:?} has invalid replica ordinal: {error}",
        process.process_key
      ))
    })?;
  if replica_ordinal < 0 {
    return Err(bootstrap_error(format!(
      "Cargo process {:?} has negative replica ordinal {replica_ordinal}",
      process.process_key
    )));
  }
  let role = required_label(process, LABEL_ROLE)?
    .parse::<CargoReplicaProcessRole>()
    .map_err(|error| bootstrap_error(error.to_string()))?;
  let container_name = required_label(process, LABEL_CONTAINER)?.to_owned();
  let essential = required_label(process, LABEL_ESSENTIAL)?
    .parse::<bool>()
    .map_err(|error| {
      bootstrap_error(format!(
        "Cargo process {:?} has invalid essential marker: {error}",
        process.process_key
      ))
    })?;
  let declaration_position = match role {
    CargoReplicaProcessRole::Sandbox => {
      if process.labels.contains_key(CARGO_CONTAINER_POSITION_LABEL) {
        return Err(bootstrap_error(format!(
          "Cargo sandbox process {:?} must not carry declaration position label {:?}",
          process.process_key, CARGO_CONTAINER_POSITION_LABEL
        )));
      }
      if container_name != SANDBOX_LOGICAL_NAME || !essential {
        return Err(bootstrap_error(format!(
          "Cargo sandbox process {:?} has contradictory logical identity",
          process.process_key
        )));
      }
      None
    }
    CargoReplicaProcessRole::App | CargoReplicaProcessRole::Init => {
      let position = required_label(process, CARGO_CONTAINER_POSITION_LABEL)?
        .parse::<usize>()
        .map_err(|error| {
          bootstrap_error(format!(
            "Cargo process {:?} has invalid declaration position: {error}",
            process.process_key
          ))
        })?;
      if container_name == SANDBOX_LOGICAL_NAME {
        return Err(bootstrap_error(format!(
          "Cargo process {:?} uses the reserved sandbox logical name outside the sandbox role",
          process.process_key
        )));
      }
      if role == CargoReplicaProcessRole::Init && !essential {
        return Err(bootstrap_error(format!(
          "Cargo init process {:?} cannot be nonessential",
          process.process_key
        )));
      }
      Some(position)
    }
  };

  Ok(ParsedCargoProcess {
    process_key: process.process_key.clone(),
    process_name: process.process_name.clone(),
    replica_key,
    replica_ordinal,
    role,
    container_name,
    declaration_position,
    essential,
    config: process.config.clone(),
    status: process.status,
  })
}

fn observed_network_mode(config: &DockerConfig) -> String {
  config
    .host_config
    .as_ref()
    .and_then(|host| host.network_mode.clone())
    .filter(|mode| !mode.is_empty())
    .unwrap_or_else(|| DEFAULT_NETWORK.to_owned())
}

fn observed_port_bindings(config: &DockerConfig) -> Option<PortMap> {
  config
    .host_config
    .as_ref()
    .and_then(|host| host.port_bindings.clone())
    .filter(|bindings| !bindings.is_empty())
}

#[derive(Debug, Clone)]
struct CargoNetworkOwnership {
  effective_mode: String,
  network_mode: Option<CargoNetworkMode>,
  port_bindings: Option<PortMap>,
  has_sandbox: bool,
  direct_installer_generation: bool,
}

fn cargo_network_mode(
  effective_mode: &str,
) -> IoResult<Option<CargoNetworkMode>> {
  if effective_mode == DEFAULT_NETWORK {
    return Ok(None);
  }
  CargoNetworkMode::new(effective_mode.to_owned())
    .map(Some)
    .map_err(|error| {
      bootstrap_error(format!(
        "observed Cargo network mode {effective_mode:?} is invalid: {error}"
      ))
    })
}

fn infer_network_ownership(
  cargo_key: &str,
  processes: &[ParsedCargoProcess],
  replica_count: usize,
) -> IoResult<CargoNetworkOwnership> {
  let sandboxes = processes
    .iter()
    .filter(|process| process.role == CargoReplicaProcessRole::Sandbox)
    .collect::<Vec<_>>();
  if !sandboxes.is_empty() {
    if sandboxes.len() != replica_count {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} has {} stable sandboxes for {replica_count} replicas",
        sandboxes.len()
      )));
    }
    let effective_mode = observed_network_mode(&sandboxes[0].config);
    if effective_mode.starts_with("container:") {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} sandbox cannot join another container namespace"
      )));
    }
    let port_bindings = observed_port_bindings(&sandboxes[0].config);
    for sandbox in sandboxes.iter().skip(1) {
      if observed_network_mode(&sandbox.config) != effective_mode
        || observed_port_bindings(&sandbox.config) != port_bindings
      {
        return Err(bootstrap_error(format!(
          "Cargo {cargo_key:?} replicas expose contradictory sandbox network ownership"
        )));
      }
    }
    return Ok(CargoNetworkOwnership {
      network_mode: cargo_network_mode(&effective_mode)?,
      effective_mode,
      port_bindings,
      has_sandbox: true,
      direct_installer_generation: false,
    });
  }

  let direct_installer_generation = processes.len() == 1
    && processes[0].role == CargoReplicaProcessRole::App
    && processes[0].process_name == format!("{cargo_key}.c");
  if direct_installer_generation {
    let effective_mode = observed_network_mode(&processes[0].config);
    let port_bindings = observed_port_bindings(&processes[0].config);
    return Ok(CargoNetworkOwnership {
      network_mode: cargo_network_mode(&effective_mode)?,
      effective_mode,
      port_bindings,
      has_sandbox: false,
      direct_installer_generation: true,
    });
  }

  if processes.iter().any(|process| {
    process.role == CargoReplicaProcessRole::App
      && !matches!(
        observed_network_mode(&process.config).as_str(),
        "host" | "none"
      )
  }) {
    return Err(bootstrap_error(format!(
      "Cargo {cargo_key:?} has no sandbox but its stable application processes do not describe a host-network Cargo"
    )));
  }
  if processes
    .iter()
    .any(|process| observed_port_bindings(&process.config).is_some())
  {
    return Err(bootstrap_error(format!(
      "Cargo {cargo_key:?} has no sandbox but exposes unexpected application-owned port bindings"
    )));
  }
  Ok(CargoNetworkOwnership {
    effective_mode: "host".to_owned(),
    network_mode: Some(
      CargoNetworkMode::new("host")
        .expect("the built-in host network mode is always valid"),
    ),
    port_bindings: None,
    has_sandbox: false,
    direct_installer_generation: false,
  })
}

fn is_runtime_environment(value: &str) -> bool {
  let name = value.split_once('=').map_or(value, |(name, _)| name);
  RUNTIME_ENVIRONMENT_NAMES.contains(&name)
}

fn is_runtime_secret_bind(value: &str) -> bool {
  value.split(':').any(|part| part == SECRET_MOUNT_TARGET)
}

fn normalize_declared_config(
  cargo_key: &str,
  process: &ParsedCargoProcess,
  ownership: &CargoNetworkOwnership,
) -> IoResult<DockerConfig> {
  let mut config = process.config.clone();
  config.networking_config = None;
  config.network_disabled = None;

  if config.hostname.as_deref()
    == Some(
      process
        .process_key
        .chars()
        .take(12)
        .collect::<String>()
        .as_str(),
    )
  {
    config.hostname = None;
  }

  if let Some(labels) = config.labels.as_mut() {
    labels.retain(|key, _| {
      !(key == "io.nanocl"
        || key.starts_with("io.nanocl.")
        || key == COMPOSE_PROJECT_LABEL)
    });
    if labels.is_empty() {
      config.labels = None;
    }
  }
  if let Some(env) = config.env.as_mut() {
    // The direct installer does not inject runtime environment. In
    // particular, its ncproxy/ncdns declarations author NANOCL_NODE and that
    // value must survive reconstruction. Reconciled Cargo containers reserve
    // and inject these names, so only that path strips them.
    if !ownership.direct_installer_generation {
      env.retain(|value| !is_runtime_environment(value));
    }
    if env.is_empty() {
      config.env = None;
    }
  }

  let mut host = config.host_config.take().unwrap_or_default();
  let observed_mode = host
    .network_mode
    .clone()
    .filter(|mode| !mode.is_empty())
    .unwrap_or_else(|| DEFAULT_NETWORK.to_owned());
  if ownership.has_sandbox {
    match observed_mode.as_str() {
      mode if mode.starts_with("container:") => host.network_mode = None,
      "host" | "none" => {}
      _ => {
        return Err(bootstrap_error(format!(
          "Cargo {cargo_key:?} container {:?} has network mode {observed_mode:?} instead of its sandbox namespace",
          process.container_name
        )));
      }
    }
  } else if ownership.direct_installer_generation {
    if observed_mode == ownership.effective_mode {
      host.network_mode = None;
    }
  } else {
    match observed_mode.as_str() {
      "host" => host.network_mode = None,
      "none" => {}
      _ => {
        return Err(bootstrap_error(format!(
          "Cargo {cargo_key:?} container {:?} has invalid host-Cargo network mode {observed_mode:?}",
          process.container_name
        )));
      }
    }
  }

  let application_ports = host
    .port_bindings
    .take()
    .filter(|bindings| !bindings.is_empty());
  if application_ports.is_some() && application_ports != ownership.port_bindings
  {
    return Err(bootstrap_error(format!(
      "Cargo {cargo_key:?} container {:?} has contradictory application-owned port bindings",
      process.container_name
    )));
  }
  if ownership.has_sandbox && application_ports.is_some() {
    return Err(bootstrap_error(format!(
      "Cargo {cargo_key:?} container {:?} duplicates sandbox-owned port bindings",
      process.container_name
    )));
  }
  if ownership.direct_installer_generation
    && let (Some(exposed), Some(port_bindings)) = (
      config.exposed_ports.as_mut(),
      ownership.port_bindings.as_ref(),
    )
  {
    exposed.retain(|port, _| !port_bindings.contains_key(port));
    if exposed.is_empty() {
      config.exposed_ports = None;
    }
  }
  host.publish_all_ports = None;
  host.auto_remove = None;
  if let Some(binds) = host.binds.as_mut() {
    binds.retain(|bind| !is_runtime_secret_bind(bind));
    if binds.is_empty() {
      host.binds = None;
    }
  }
  config.host_config = Some(host);
  Ok(config)
}

fn validate_positions(
  cargo_key: &str,
  collection: &str,
  declarations: &BTreeMap<usize, ContainerSpec>,
) -> IoResult<()> {
  for (expected, position) in declarations.keys().copied().enumerate() {
    if position != expected {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} {collection} positions must be contiguous from zero; found {position} where {expected} was expected"
      )));
    }
  }
  Ok(())
}

fn reconstructed_actual(processes: &[ParsedCargoProcess]) -> ObjPsStatusKind {
  let required = processes
    .iter()
    .filter(|process| {
      process.role == CargoReplicaProcessRole::Sandbox
        || (process.role == CargoReplicaProcessRole::App && process.essential)
    })
    .collect::<Vec<_>>();
  if required
    .iter()
    .any(|process| process.status == Some(ContainerStateStatusEnum::RESTARTING))
  {
    ObjPsStatusKind::Fail
  } else if !required.is_empty()
    && required
      .iter()
      .all(|process| process.status == Some(ContainerStateStatusEnum::RUNNING))
  {
    ObjPsStatusKind::Start
  } else {
    ObjPsStatusKind::Stop
  }
}

/// Reduce stable observed runtime state without requiring declaration-order
/// metadata. This keeps existing desired Cargo rows compatible with processes
/// created before the position label was introduced.
pub(super) fn observed_actual_status(
  observed: &[CargoBootstrapProcess],
) -> Option<ObjPsStatusKind> {
  let required = observed
    .iter()
    .filter(|process| !is_rollout_artifact(&process.process_name))
    .filter(|process| {
      match process.labels.get(LABEL_ROLE).map(String::as_str) {
        Some("sandbox") => true,
        Some("app") => process
          .labels
          .get(LABEL_ESSENTIAL)
          .and_then(|value| value.parse::<bool>().ok())
          .unwrap_or(true),
        Some("init") => false,
        // Nightly installer containers predate canonical role labels and use
        // this application marker.
        None => process.labels.contains_key("io.nanocl.not-init-c"),
        Some(_) => false,
      }
    })
    .collect::<Vec<_>>();
  if required.is_empty() {
    return None;
  }
  if required
    .iter()
    .any(|process| process.status == Some(ContainerStateStatusEnum::RESTARTING))
  {
    Some(ObjPsStatusKind::Fail)
  } else if required
    .iter()
    .all(|process| process.status == Some(ContainerStateStatusEnum::RUNNING))
  {
    Some(ObjPsStatusKind::Start)
  } else {
    Some(ObjPsStatusKind::Stop)
  }
}

/// Deterministically rebuild a final Cargo declaration from one stable
/// observed generation. Pending and retained rollout artifacts are ignored.
pub(super) fn reconstruct_cargo(
  cargo_key: &str,
  observed: &[CargoBootstrapProcess],
) -> IoResult<Option<ReconstructedCargo>> {
  let stable = observed
    .iter()
    .filter(|process| !is_rollout_artifact(&process.process_name))
    .map(|process| parse_process(cargo_key, process))
    .collect::<IoResult<Vec<_>>>()?;
  if stable.is_empty() {
    return Ok(None);
  }

  let resource_key = cargo_key.parse::<ResourceKey>().map_err(|error| {
    bootstrap_error(format!("invalid Cargo key {cargo_key:?}: {error}"))
  })?;
  let mut replicas_by_ordinal = BTreeMap::<i32, uuid::Uuid>::new();
  let mut ordinals_by_replica = HashMap::<uuid::Uuid, i32>::new();
  let mut runtime_identities = HashSet::new();
  let mut positions_by_replica =
    HashMap::<uuid::Uuid, BTreeSet<(u8, usize)>>::new();
  for process in &stable {
    if let Some(existing) =
      replicas_by_ordinal.insert(process.replica_ordinal, process.replica_key)
      && existing != process.replica_key
    {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} replica ordinal {} identifies both {existing} and {}",
        process.replica_ordinal, process.replica_key
      )));
    }
    if let Some(existing) =
      ordinals_by_replica.insert(process.replica_key, process.replica_ordinal)
      && existing != process.replica_ordinal
    {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} replica {} identifies both ordinal {existing} and {}",
        process.replica_key, process.replica_ordinal
      )));
    }
    if !runtime_identities.insert((
      process.replica_key,
      process.role,
      process.container_name.clone(),
    )) {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} replica {} has duplicate stable {} process {:?}",
        process.replica_key, process.role, process.container_name
      )));
    }
    if let Some(position) = process.declaration_position {
      let role = match process.role {
        CargoReplicaProcessRole::Init => 0,
        CargoReplicaProcessRole::App => 1,
        CargoReplicaProcessRole::Sandbox => unreachable!(),
      };
      if !positions_by_replica
        .entry(process.replica_key)
        .or_default()
        .insert((role, position))
      {
        return Err(bootstrap_error(format!(
          "Cargo {cargo_key:?} replica {} has duplicate {} declaration position {position}",
          process.replica_key, process.role
        )));
      }
    }
  }
  for (expected, ordinal) in replicas_by_ordinal.keys().copied().enumerate() {
    if ordinal != expected as i32 {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} replica ordinals must be contiguous from zero; found {ordinal} where {expected} was expected"
      )));
    }
  }

  let ownership =
    infer_network_ownership(cargo_key, &stable, replicas_by_ordinal.len())?;
  let mut init_containers = BTreeMap::<usize, ContainerSpec>::new();
  let mut containers = BTreeMap::<usize, ContainerSpec>::new();
  let mut logical_positions = HashMap::<(u8, String), usize>::new();
  for process in stable
    .iter()
    .filter(|process| process.role != CargoReplicaProcessRole::Sandbox)
  {
    let position = process
      .declaration_position
      .expect("declared Cargo roles always have a parsed position");
    let role = if process.role == CargoReplicaProcessRole::Init {
      0
    } else {
      1
    };
    if let Some(existing) =
      logical_positions.insert((role, process.container_name.clone()), position)
      && existing != position
    {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} logical {} container {:?} has positions {existing} and {position}",
        process.role, process.container_name
      )));
    }
    let declaration = ContainerSpec {
      name: process.container_name.clone(),
      essential: process.essential,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::default(),
      container_config: normalize_declared_config(
        cargo_key, process, &ownership,
      )?,
    };
    let declarations = if process.role == CargoReplicaProcessRole::Init {
      &mut init_containers
    } else {
      &mut containers
    };
    if let Some(existing) = declarations.get(&position) {
      if existing != &declaration {
        return Err(bootstrap_error(format!(
          "Cargo {cargo_key:?} declaration position {position} has contradictory observed {} definitions",
          process.role
        )));
      }
    } else {
      declarations.insert(position, declaration);
    }
  }
  validate_positions(cargo_key, "InitContainers", &init_containers)?;
  validate_positions(cargo_key, "Containers", &containers)?;

  let expected_positions = init_containers
    .keys()
    .copied()
    .map(|position| (0, position))
    .chain(containers.keys().copied().map(|position| (1, position)))
    .collect::<BTreeSet<_>>();
  for replica_key in ordinals_by_replica.keys() {
    let observed_positions = positions_by_replica
      .get(replica_key)
      .cloned()
      .unwrap_or_default();
    if observed_positions != expected_positions {
      return Err(bootstrap_error(format!(
        "Cargo {cargo_key:?} replica {replica_key} does not contain the complete declared app/init position set"
      )));
    }
  }

  let spec = CargoSpec {
    name: resource_key.name().to_owned(),
    metadata: None,
    replicas: replicas_by_ordinal.len(),
    network_mode: ownership.network_mode.clone(),
    port_bindings: ownership.port_bindings.clone(),
    secrets: Vec::new(),
    placement: None,
    resource_requirement: None,
    init_containers: init_containers.into_values().collect(),
    containers: containers.into_values().collect(),
  };
  spec.validate().map_err(|error| {
    bootstrap_error(format!(
      "reconstructed Cargo {cargo_key:?} is invalid: {error}"
    ))
  })?;
  let processes = stable
    .iter()
    .map(|process| ReconstructedCargoProcess {
      process_key: process.process_key.clone(),
      replica_key: process.replica_key,
      replica_ordinal: process.replica_ordinal,
      role: process.role,
      container_name: process.container_name.clone(),
      essential: process.essential,
    })
    .collect();
  Ok(Some(ReconstructedCargo {
    spec,
    processes,
    actual: reconstructed_actual(&stable),
    direct_installer_generation: ownership.direct_installer_generation,
  }))
}

#[cfg(test)]
mod tests {
  use bollard_next::models::{
    ContainerConfig, ContainerState, EmptyObject, HostConfig, PortBinding,
  };

  use super::*;

  const KEY: &str = "system.bootstrap";

  fn replica(value: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(value)
  }

  fn process(
    replica_key: uuid::Uuid,
    ordinal: i32,
    role: CargoReplicaProcessRole,
    logical_name: &str,
    position: Option<&str>,
    process_name: &str,
  ) -> CargoBootstrapProcess {
    let mut labels = HashMap::from([
      (LABEL_CARGO_KEY.to_owned(), KEY.to_owned()),
      (LABEL_REPLICA.to_owned(), replica_key.to_string()),
      (LABEL_REPLICA_ORDINAL.to_owned(), ordinal.to_string()),
      (LABEL_ROLE.to_owned(), role.to_string()),
      (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
      (LABEL_ESSENTIAL.to_owned(), "true".to_owned()),
    ]);
    if let Some(position) = position {
      labels.insert(
        CARGO_CONTAINER_POSITION_LABEL.to_owned(),
        position.to_owned(),
      );
    }
    let network_mode = if role == CargoReplicaProcessRole::Sandbox {
      DEFAULT_NETWORK.to_owned()
    } else {
      format!("container:sandbox-{ordinal}")
    };
    CargoBootstrapProcess {
      process_key: format!("{ordinal}-{role}-{logical_name}"),
      process_name: process_name.to_owned(),
      labels: labels.clone(),
      config: DockerConfig {
        image: Some(
          if role == CargoReplicaProcessRole::Sandbox {
            "pause:latest"
          } else {
            "alpine:latest"
          }
          .to_owned(),
        ),
        labels: Some(labels),
        host_config: Some(HostConfig {
          network_mode: Some(network_mode),
          ..Default::default()
        }),
        ..Default::default()
      },
      status: Some(ContainerStateStatusEnum::RUNNING),
    }
  }

  fn sandbox(replica_key: uuid::Uuid, ordinal: i32) -> CargoBootstrapProcess {
    process(
      replica_key,
      ordinal,
      CargoReplicaProcessRole::Sandbox,
      SANDBOX_LOGICAL_NAME,
      None,
      &format!("sandbox-system.bootstrap-r{ordinal}.c"),
    )
  }

  #[test]
  fn docker_inspect_is_projected_into_the_reconstruction_observation() {
    let labels = HashMap::from([("io.nanocl.c".to_owned(), KEY.to_owned())]);
    let observed =
      CargoBootstrapProcess::from_inspect(&ContainerInspectResponse {
        id: Some("0123456789abcdef".to_owned()),
        name: Some("/system.bootstrap.c".to_owned()),
        config: Some(ContainerConfig {
          image: Some("alpine:latest".to_owned()),
          labels: Some(labels.clone()),
          ..Default::default()
        }),
        host_config: Some(HostConfig {
          network_mode: Some(DEFAULT_NETWORK.to_owned()),
          ..Default::default()
        }),
        state: Some(ContainerState {
          status: Some(ContainerStateStatusEnum::RUNNING),
          ..Default::default()
        }),
        ..Default::default()
      })
      .unwrap();

    assert_eq!(observed.process_key, "0123456789abcdef");
    assert_eq!(observed.process_name, "system.bootstrap.c");
    assert_eq!(observed.labels, labels);
    assert_eq!(observed.config.image.as_deref(), Some("alpine:latest"));
    assert_eq!(
      observed
        .config
        .host_config
        .as_ref()
        .and_then(|host| host.network_mode.as_deref()),
      Some(DEFAULT_NETWORK)
    );
    assert_eq!(observed.status, Some(ContainerStateStatusEnum::RUNNING));
  }

  fn app(
    replica_key: uuid::Uuid,
    ordinal: i32,
    name: &str,
    position: &str,
  ) -> CargoBootstrapProcess {
    process(
      replica_key,
      ordinal,
      CargoReplicaProcessRole::App,
      name,
      Some(position),
      &format!("system.bootstrap-r{ordinal}-{name}.c"),
    )
  }

  fn init(
    replica_key: uuid::Uuid,
    ordinal: i32,
    name: &str,
    position: &str,
  ) -> CargoBootstrapProcess {
    process(
      replica_key,
      ordinal,
      CargoReplicaProcessRole::Init,
      name,
      Some(position),
      &format!("init-system.bootstrap-r{ordinal}-{name}.c"),
    )
  }

  #[test]
  fn one_replica_one_application_reconstructs_one_container() {
    let replica = replica(1);
    let reconstructed = reconstruct_cargo(
      KEY,
      &[sandbox(replica, 0), app(replica, 0, "api", "0")],
    )
    .unwrap()
    .unwrap();

    assert_eq!(reconstructed.spec.replicas, 1);
    assert_eq!(reconstructed.spec.containers.len(), 1);
    assert_eq!(reconstructed.spec.containers[0].name, "api");
    assert!(reconstructed.spec.secrets.is_empty());
    assert!(reconstructed.spec.containers[0].secrets.is_empty());
    assert!(reconstructed.spec.containers[0].image_pull_secret.is_none());
    assert_eq!(
      reconstructed.spec.containers[0].image_pull_policy,
      ImagePullPolicy::IfNotPresent
    );
    assert_eq!(reconstructed.actual, ObjPsStatusKind::Start);
  }

  #[test]
  fn reconciler_runtime_metadata_is_removed_but_user_config_is_preserved() {
    let replica = replica(1);
    let mut app = app(replica, 0, "api", "0");
    app
      .config
      .labels
      .as_mut()
      .unwrap()
      .insert("example.owner".to_owned(), "platform".to_owned());
    app.config.env = Some(vec![
      "KEEP=yes".to_owned(),
      "NANOCL_CARGO_INSTANCE=0".to_owned(),
    ]);
    app.config.host_config.as_mut().unwrap().binds = Some(vec![
      "/tmp/generated:/opt/nanocl.io/secrets:ro".to_owned(),
      "/data:/data".to_owned(),
    ]);
    let reconstructed = reconstruct_cargo(KEY, &[sandbox(replica, 0), app])
      .unwrap()
      .unwrap();
    let config = &reconstructed.spec.containers[0].container_config;

    assert_eq!(
      config
        .labels
        .as_ref()
        .unwrap()
        .get("example.owner")
        .map(String::as_str),
      Some("platform")
    );
    assert!(
      config
        .labels
        .as_ref()
        .unwrap()
        .keys()
        .all(|key| !key.starts_with("io.nanocl"))
    );
    assert_eq!(
      config.env.as_deref(),
      Some(["KEEP=yes".to_owned()].as_slice())
    );
    assert_eq!(
      config.host_config.as_ref().unwrap().binds.as_deref(),
      Some(["/data:/data".to_owned()].as_slice())
    );
  }

  #[test]
  fn application_and_init_order_uses_independent_explicit_positions() {
    let replica = replica(1);
    let reconstructed = reconstruct_cargo(
      KEY,
      &[
        app(replica, 0, "worker", "1"),
        init(replica, 0, "migrate", "1"),
        sandbox(replica, 0),
        init(replica, 0, "prepare", "0"),
        app(replica, 0, "api", "0"),
      ],
    )
    .unwrap()
    .unwrap();

    assert_eq!(
      reconstructed
        .spec
        .containers
        .iter()
        .map(|container| container.name.as_str())
        .collect::<Vec<_>>(),
      ["api", "worker"]
    );
    assert_eq!(
      reconstructed
        .spec
        .init_containers
        .iter()
        .map(|container| container.name.as_str())
        .collect::<Vec<_>>(),
      ["prepare", "migrate"]
    );
  }

  #[test]
  fn replica_copies_deduplicate_into_two_ordered_declarations() {
    let first = replica(1);
    let second = replica(2);
    let reconstructed = reconstruct_cargo(
      KEY,
      &[
        sandbox(first, 0),
        app(first, 0, "sidecar", "1"),
        app(first, 0, "api", "0"),
        sandbox(second, 1),
        app(second, 1, "api", "0"),
        app(second, 1, "sidecar", "1"),
      ],
    )
    .unwrap()
    .unwrap();

    assert_eq!(reconstructed.spec.replicas, 2);
    assert_eq!(reconstructed.spec.containers.len(), 2);
    assert_eq!(reconstructed.spec.containers[0].name, "api");
    assert_eq!(reconstructed.spec.containers[1].name, "sidecar");
  }

  #[test]
  fn sandbox_owns_network_and_ports_but_is_not_a_user_container() {
    let replica = replica(1);
    let bindings = HashMap::from([(
      "8080/tcp".to_owned(),
      Some(vec![PortBinding {
        host_ip: Some("127.0.0.1".to_owned()),
        host_port: Some("8080".to_owned()),
      }]),
    )]);
    let mut sandbox = sandbox(replica, 0);
    let host = sandbox.config.host_config.as_mut().unwrap();
    host.network_mode = Some("private-api".to_owned());
    host.port_bindings = Some(bindings.clone());
    let reconstructed =
      reconstruct_cargo(KEY, &[sandbox, app(replica, 0, "api", "0")])
        .unwrap()
        .unwrap();

    assert_eq!(
      reconstructed
        .spec
        .network_mode
        .as_ref()
        .map(|mode| mode.as_str()),
      Some("private-api")
    );
    assert_eq!(reconstructed.spec.port_bindings, Some(bindings));
    assert!(reconstructed.spec.init_containers.is_empty());
    assert_eq!(reconstructed.spec.containers.len(), 1);
    assert_eq!(
      reconstructed.spec.containers[0]
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.network_mode.as_deref()),
      None
    );
    assert!(reconstructed.processes.iter().any(|process| {
      process.role == CargoReplicaProcessRole::Sandbox
        && process.container_name == SANDBOX_LOGICAL_NAME
    }));
  }

  #[test]
  fn duplicate_logical_position_is_rejected() {
    let replica = replica(1);
    let error = reconstruct_cargo(
      KEY,
      &[
        sandbox(replica, 0),
        app(replica, 0, "api", "0"),
        app(replica, 0, "worker", "0"),
      ],
    )
    .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("duplicate app declaration position 0")
    );
  }

  #[test]
  fn missing_and_invalid_positions_are_rejected() {
    let replica = replica(1);
    let missing = process(
      replica,
      0,
      CargoReplicaProcessRole::App,
      "api",
      None,
      "system.bootstrap-r0-api.c",
    );
    let error =
      reconstruct_cargo(KEY, &[sandbox(replica, 0), missing]).unwrap_err();
    assert!(error.to_string().contains(CARGO_CONTAINER_POSITION_LABEL));

    let error = reconstruct_cargo(
      KEY,
      &[sandbox(replica, 0), app(replica, 0, "api", "-1")],
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid declaration position"));
  }

  #[test]
  fn pending_and_retained_rollout_processes_are_excluded() {
    let replica = replica(1);
    let mut candidate = app(replica, 0, "candidate-copy", "1");
    candidate.process_name = "candidate-system.bootstrap-r0-copy.c".to_owned();
    candidate.labels.remove(CARGO_CONTAINER_POSITION_LABEL);
    let mut retained = app(replica, 0, "retained-copy", "1");
    retained.process_name = "tmp-system.bootstrap-r0-copy.c".to_owned();
    retained.labels.remove(CARGO_CONTAINER_POSITION_LABEL);
    let reconstructed = reconstruct_cargo(
      KEY,
      &[
        candidate,
        retained,
        sandbox(replica, 0),
        app(replica, 0, "api", "0"),
      ],
    )
    .unwrap()
    .unwrap();
    assert_eq!(reconstructed.spec.containers.len(), 1);
    assert_eq!(reconstructed.spec.containers[0].name, "api");

    let mut only_candidate = app(replica, 0, "api", "0");
    only_candidate.process_name = "candidate-only.c".to_owned();
    assert!(reconstruct_cargo(KEY, &[only_candidate]).unwrap().is_none());
  }

  #[test]
  fn direct_installer_projection_lifts_default_network_and_ports() {
    let replica = replica(1);
    let bindings = HashMap::from([(
      "9443/tcp".to_owned(),
      Some(vec![PortBinding {
        host_ip: Some("0.0.0.0".to_owned()),
        host_port: Some("9443".to_owned()),
      }]),
    )]);
    let mut app = app(replica, 0, "bootstrap", "0");
    app.process_name = format!("{KEY}.c");
    app.config.env = Some(vec!["NANOCL_NODE=node-a".to_owned()]);
    app.config.exposed_ports = Some(HashMap::from([(
      "9443/tcp".to_owned(),
      EmptyObject::default(),
    )]));
    let host = app.config.host_config.as_mut().unwrap();
    host.network_mode = Some(DEFAULT_NETWORK.to_owned());
    host.port_bindings = Some(bindings.clone());
    let reconstructed = reconstruct_cargo(KEY, &[app]).unwrap().unwrap();

    assert!(reconstructed.direct_installer_generation);
    assert_eq!(reconstructed.spec.network_mode, None);
    assert_eq!(reconstructed.spec.port_bindings, Some(bindings));
    assert_eq!(
      reconstructed.spec.containers[0]
        .container_config
        .env
        .as_deref(),
      Some(["NANOCL_NODE=node-a".to_owned()].as_slice())
    );
    assert_eq!(
      reconstructed.spec.containers[0]
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.network_mode.as_ref()),
      None
    );
    assert_eq!(
      reconstructed.spec.containers[0]
        .container_config
        .exposed_ports,
      None
    );
    assert_eq!(
      reconstructed.spec.containers[0]
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.port_bindings.as_ref()),
      None
    );
  }
}
