use std::{
  collections::{HashMap, HashSet},
  hash::{DefaultHasher, Hash, Hasher},
  time::{Duration, Instant},
};

use bollard_next::container::{
  InspectContainerOptions, RemoveContainerOptions, RenameContainerOptions,
  StartContainerOptions, StopContainerOptions,
};
use futures::future::try_join_all;
use nanocld_client::{ConnectOpts, NanocldClient};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::{CargoNetworkMode, Config as DockerConfig, ContainerSpec},
  generic::{GenericClause, GenericFilter, ImagePullPolicy},
  node::{CargoReplicaTask, DeleteNodeCargoParams},
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsHealthStatusKind, ObjPsStatusKind},
};

use crate::{
  models::{
    CargoDb, CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
    CompileDeclaredOpts, EnsureExistingSlotOpts, NewCargoReplicaProcessDb,
    NodeDb, ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb, ProcessUpdateDb,
    SystemState,
  },
  repositories::generic::*,
  utils,
};

use super::{
  cargo_compiler::{
    CargoContainerRole, CargoRuntimeMetadata, ContainerCompileInput,
    SandboxCompileInput, compile_container, compile_sandbox,
  },
  network::DEFAULT_NETWORK,
};

mod identity;
mod init;
mod readiness;
mod replacement;
mod restart;

use identity::{
  is_rollout_process_name, process_identity, replica_ordinal,
  validate_observed_identities,
};
use init::{create_candidate_containers, reconcile_containers};

pub use readiness::has_container_healthcheck;
pub(crate) use readiness::{all_desired_replicas_ready, promotion_actions};
use readiness::{
  healthcheck_enabled, observed_container_ready, observed_container_running,
  required_candidate_topology_complete, requires_steady_state_readiness,
  restart_required_slots_ready,
};
use replacement::{
  requires_removal_before_replacement, requires_stop_before_replacement,
  validate_local_fixed_port_ownership,
};
use restart::{CargoRestartPlan, plan_cargo_restart};

const SANDBOX_LOGICAL_NAME: &str = "_sandbox";
const DEFAULT_SANDBOX_IMAGE: &str = "registry.k8s.io/pause:3.10";
const READY_TIMEOUT: Duration = Duration::from_secs(300);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(500);
const ROUTE_HANDOFF_GRACE: Duration = Duration::from_secs(4);
const RUNTIME_DELETION_ORDER: [CargoReplicaProcessRole; 3] = [
  CargoReplicaProcessRole::App,
  CargoReplicaProcessRole::Init,
  CargoReplicaProcessRole::Sandbox,
];

#[derive(Clone, Debug)]
struct RuntimeSlot {
  mapping: CargoReplicaProcessDb,
  process: Process,
  role: CargoReplicaProcessRole,
  container_name: String,
  essential: bool,
}

struct ReplicaRuntime {
  slots: Vec<RuntimeSlot>,
}

struct PreparedReplicaUpdate {
  task: CargoReplicaTask,
  old: Vec<RuntimeSlot>,
  candidate: ReplicaRuntime,
  stop_first: bool,
}

fn sandbox_image() -> String {
  std::env::var("NANOCL_CARGO_SANDBOX_IMAGE")
    .ok()
    .filter(|image| !image.trim().is_empty())
    .unwrap_or_else(|| DEFAULT_SANDBOX_IMAGE.to_owned())
}

fn cargo_network_mode(cargo: &Cargo) -> &str {
  cargo
    .spec
    .network_mode
    .as_ref()
    .map_or(DEFAULT_NETWORK, CargoNetworkMode::as_str)
}

fn runtime_metadata<'a>(
  cargo: &'a Cargo,
  replica_id: &'a str,
  ordinal: u32,
  state: &'a SystemState,
) -> CargoRuntimeMetadata<'a> {
  CargoRuntimeMetadata {
    cargo_key: &cargo.spec.cargo_key,
    namespace: &cargo.namespace_name,
    replica_id,
    replica_ordinal: ordinal,
    node_name: &state.inner.config.hostname,
    node_address: &state.inner.config.gateway,
  }
}

fn effective_secret_refs(
  cargo: &Cargo,
  container: &ContainerSpec,
) -> Vec<String> {
  let mut seen = HashSet::new();
  cargo
    .spec
    .secrets
    .iter()
    .chain(&container.secrets)
    .filter(|secret| seen.insert((*secret).clone()))
    .cloned()
    .collect()
}

fn runtime_path_component(value: &str) -> String {
  let mut hasher = DefaultHasher::new();
  value.hash(&mut hasher);
  format!("{:016x}", hasher.finish())
}

fn overlay_environment(base: Vec<String>, overlay: Vec<String>) -> Vec<String> {
  let mut values = Vec::new();
  let mut positions = HashMap::<String, usize>::new();
  for value in base.into_iter().chain(overlay) {
    let name = value
      .split_once('=')
      .map_or(value.as_str(), |(name, _)| name)
      .to_owned();
    if let Some(position) = positions.get(&name).copied() {
      values[position] = value;
    } else {
      positions.insert(name, values.len());
      values.push(value);
    }
  }
  values
}

async fn validated_secret_env(
  cargo: &Cargo,
  declared: &ContainerSpec,
  state: &SystemState,
) -> IoResult<(Option<Vec<String>>, Vec<String>)> {
  let secrets = Some(effective_secret_refs(cargo, declared));
  let secret_env = utils::secret::load_env_secrets(&secrets, state).await?;
  if let Some(variable) = secret_env.iter().find_map(|value| {
    let variable = value.split_once('=').map_or(value.as_str(), |item| item.0);
    variable.starts_with("NANOCL_").then_some(variable)
  }) {
    return Err(IoError::invalid_data(
      "CargoSecrets",
      &format!(
        "Cargo {} container {:?} secret defines reserved environment variable {variable:?}",
        cargo.spec.cargo_key, declared.name
      ),
    ));
  }
  utils::secret::validate_tls_secrets(&secrets, state).await?;
  Ok((secrets, secret_env))
}

async fn inject_secrets(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  declared: &ContainerSpec,
  role: CargoReplicaProcessRole,
  attempt_id: &str,
  mut config: DockerConfig,
  state: &SystemState,
) -> IoResult<DockerConfig> {
  let (secrets, secret_env) =
    validated_secret_env(cargo, declared, state).await?;
  config.env = Some(overlay_environment(
    secret_env,
    config.env.take().unwrap_or_default(),
  ));

  let secret_key = format!(
    "{}-{}-{}-{}-{}",
    runtime_path_component(&cargo.spec.cargo_key),
    task.key,
    runtime_path_component(attempt_id),
    role,
    runtime_path_component(&declared.name)
  );
  let secret_dir = utils::secret::create_tls_secrets(
    &secret_key,
    &ProcessKind::Cargo,
    &secrets,
    state,
  )
  .await?;
  let mut host_config = config.host_config.take().unwrap_or_default();
  let mut binds = host_config.binds.take().unwrap_or_default();
  binds.push(format!("{secret_dir}:/opt/nanocl.io/secrets:ro"));
  host_config.binds = Some(binds);
  config.host_config = Some(host_config);
  Ok(config)
}

async fn internal_gateway(
  cargo: &Cargo,
  state: &SystemState,
) -> IoResult<String> {
  super::generic::resolve_internal_gateway(cargo_network_mode(cargo), state)
    .await
}

async fn compile_declared(
  opts: CompileDeclaredOpts<'_>,
) -> IoResult<DockerConfig> {
  let CompileDeclaredOpts {
    cargo,
    task,
    declared,
    role,
    sandbox_id,
    attempt_id,
    gateway,
    state,
  } = opts;
  let config = compile_declared_base(
    cargo, task, declared, role, sandbox_id, gateway, state,
  )?;
  inject_secrets(cargo, task, declared, role, attempt_id, config, state).await
}

fn declared_container_position(
  cargo: &Cargo,
  declared: &ContainerSpec,
  role: CargoReplicaProcessRole,
) -> IoResult<usize> {
  let declarations = match role {
    CargoReplicaProcessRole::App => &cargo.spec.containers,
    CargoReplicaProcessRole::Init => &cargo.spec.init_containers,
    CargoReplicaProcessRole::Sandbox => {
      return Err(IoError::other(
        "CargoRuntime",
        "The internal sandbox has no user declaration position",
      ));
    }
  };
  declarations
    .iter()
    .position(|candidate| candidate.name == declared.name)
    .ok_or_else(|| {
      IoError::other(
        "CargoRuntime",
        &format!(
          "Cargo {} container {:?} is not present in its declared {role} collection",
          cargo.spec.cargo_key, declared.name
        ),
      )
    })
}

fn compile_declared_base(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  declared: &ContainerSpec,
  role: CargoReplicaProcessRole,
  sandbox_id: Option<&str>,
  gateway: &str,
  state: &SystemState,
) -> IoResult<DockerConfig> {
  let replica_id = task.key.to_string();
  let ordinal = replica_ordinal(task)?;
  let compiler_role = match role {
    CargoReplicaProcessRole::App => CargoContainerRole::Application,
    CargoReplicaProcessRole::Init => CargoContainerRole::Init,
    CargoReplicaProcessRole::Sandbox => {
      return Err(IoError::other(
        "CargoRuntime",
        "A declared container cannot use the sandbox role",
      ));
    }
  };
  let declaration_position =
    declared_container_position(cargo, declared, role)?;
  let compiled = compile_container(ContainerCompileInput {
    declared,
    declaration_position,
    cargo_network_mode: cargo.spec.network_mode.as_ref(),
    sandbox_id,
    role: compiler_role,
    runtime: runtime_metadata(cargo, &replica_id, ordinal, state),
    internal_gateway: Some(gateway),
  })?;
  Ok(compiled.config)
}

async fn compile_sandbox_config(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  state: &SystemState,
) -> IoResult<Option<DockerConfig>> {
  let replica_id = task.key.to_string();
  let ordinal = replica_ordinal(task)?;
  let image = sandbox_image();
  Ok(
    compile_sandbox(SandboxCompileInput {
      cargo_network_mode: cargo.spec.network_mode.as_ref(),
      port_bindings: cargo.spec.port_bindings.as_ref(),
      sandbox_image: &image,
      runtime: runtime_metadata(cargo, &replica_id, ordinal, state),
    })?
    .map(|compiled| compiled.config),
  )
}

async fn download_declared_images(
  cargo: &Cargo,
  state: &SystemState,
) -> IoResult<()> {
  for declared in cargo
    .spec
    .init_containers
    .iter()
    .chain(&cargo.spec.containers)
  {
    let image =
      declared.container_config.image.as_deref().ok_or_else(|| {
        IoError::invalid_data("CargoRuntime", "Missing image")
      })?;
    super::image::download(
      image,
      declared.image_pull_secret.clone(),
      declared.image_pull_policy.clone(),
      cargo,
      state,
    )
    .await?;
  }
  Ok(())
}

async fn preflight_replica(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  state: &SystemState,
) -> IoResult<()> {
  let mode = cargo_network_mode(cargo).to_owned();
  super::network::ensure_networks([mode], state).await?;
  let sandbox = compile_sandbox_config(cargo, task, state).await?;
  if sandbox.is_some() {
    let image = sandbox_image();
    super::image::download(
      &image,
      None,
      ImagePullPolicy::IfNotPresent,
      cargo,
      state,
    )
    .await?;
  }
  download_declared_images(cargo, state).await?;
  let gateway = internal_gateway(cargo, state).await?;
  let placeholder = sandbox.as_ref().map(|_| "preflight-sandbox");
  for declared in &cargo.spec.init_containers {
    let _ = compile_declared_base(
      cargo,
      task,
      declared,
      CargoReplicaProcessRole::Init,
      placeholder,
      &gateway,
      state,
    )?;
    validated_secret_env(cargo, declared, state).await?;
  }
  for declared in &cargo.spec.containers {
    let _ = compile_declared_base(
      cargo,
      task,
      declared,
      CargoReplicaProcessRole::App,
      placeholder,
      &gateway,
      state,
    )?;
    validated_secret_env(cargo, declared, state).await?;
  }
  Ok(())
}

fn process_name(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  role: CargoReplicaProcessRole,
  logical_name: &str,
  pending: bool,
) -> String {
  let short_id = utils::key::generate_short_id(6);
  let pending_prefix = if pending { "candidate-" } else { "" };
  let prefix = match role {
    CargoReplicaProcessRole::Sandbox => "sandbox-",
    CargoReplicaProcessRole::Init => "init-",
    CargoReplicaProcessRole::App => "",
  };
  let logical_name = runtime_path_component(logical_name);
  format!(
    "{pending_prefix}{prefix}{}.{}-r{}-{logical_name}-{short_id}.c",
    cargo.namespace_name, cargo.spec.name, task.ordinal
  )
}

async fn ensure_mapping(
  task: &CargoReplicaTask,
  role: CargoReplicaProcessRole,
  container_name: &str,
  essential: bool,
  state: &SystemState,
) -> IoResult<CargoReplicaProcessDb> {
  let mappings =
    CargoReplicaProcessDb::list_by_replica(task.key, &state.inner.pool).await?;
  if let Some(mapping) = mappings.into_iter().find(|mapping| {
    mapping.role == role && mapping.container_name == container_name
  }) {
    return Ok(mapping);
  }
  CargoReplicaProcessDb::create(
    NewCargoReplicaProcessDb::new(task.key, container_name, role, essential),
    &state.inner.pool,
  )
  .await
}

async fn mapped_process(
  mapping: &CargoReplicaProcessDb,
  state: &SystemState,
) -> IoResult<Option<Process>> {
  let Some(process_key) = mapping.process_key.as_deref() else {
    return Ok(None);
  };
  match ProcessDb::transform_read_by_pk(process_key, &state.inner.pool).await {
    Ok(process) => Ok(Some(process)),
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      CargoReplicaProcessDb::clear_process(mapping.key, &state.inner.pool)
        .await?;
      Ok(None)
    }
    Err(error) => Err(error),
  }
}

async fn create_process(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  role: CargoReplicaProcessRole,
  container_name: &str,
  config: &DockerConfig,
  pending: bool,
  state: &SystemState,
) -> IoResult<Process> {
  let name = process_name(cargo, task, role, container_name, pending);
  super::process::create(
    &ProcessKind::Cargo,
    &name,
    &cargo.spec.cargo_key,
    config,
    state,
  )
  .await
}

async fn attach_process(
  mapping: &CargoReplicaProcessDb,
  process: &Process,
  state: &SystemState,
) -> IoResult<()> {
  if let Err(error) = CargoReplicaProcessDb::attach_process(
    mapping.key,
    &process.key,
    &state.inner.pool,
  )
  .await
  {
    let _ = super::process::delete_instance(
      &process.key,
      Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
      }),
      state,
    )
    .await;
    return Err(error);
  }
  Ok(())
}

async fn inspect_process(
  process_key: &str,
  state: &SystemState,
) -> IoResult<bollard_next::service::ContainerInspectResponse> {
  Ok(
    state
      .inner
      .docker_api
      .inspect_container(process_key, None::<InspectContainerOptions>)
      .await
      .map_err(|error| error.map_err_context(|| "InspectCargoProcess"))?,
  )
}

async fn start_process(process: &Process, state: &SystemState) -> IoResult<()> {
  let inspected = inspect_process(&process.key, state).await?;
  if inspected
    .state
    .as_ref()
    .and_then(|state| state.running)
    .unwrap_or_default()
  {
    return Ok(());
  }
  Ok(
    state
      .inner
      .docker_api
      .start_container(&process.key, None::<StartContainerOptions<String>>)
      .await
      .map_err(|error| error.map_err_context(|| "StartCargoProcess"))?,
  )
}

async fn stop_process(process: &Process, state: &SystemState) -> IoResult<()> {
  let inspected = inspect_process(&process.key, state).await?;
  if !inspected
    .state
    .as_ref()
    .and_then(|state| state.running)
    .unwrap_or_default()
  {
    return Ok(());
  }
  Ok(
    state
      .inner
      .docker_api
      .stop_container(&process.key, None::<StopContainerOptions>)
      .await
      .map_err(|error| error.map_err_context(|| "StopCargoProcess"))?,
  )
}

async fn wait_until_ready(
  slots: &[RuntimeSlot],
  state: &SystemState,
) -> IoResult<bool> {
  let deadline = Instant::now() + READY_TIMEOUT;
  let mut any_healthcheck = false;
  loop {
    let mut pending = Vec::new();
    for slot in slots
      .iter()
      .filter(|slot| requires_steady_state_readiness(slot.role, slot.essential))
    {
      let inspected = inspect_process(&slot.process.key, state).await?;
      let container_state = inspected.state.as_ref();
      let running = container_state
        .and_then(|state| state.running)
        .unwrap_or_default();
      if !running {
        let status = container_state
          .and_then(|state| state.status.as_ref())
          .map(ToString::to_string)
          .unwrap_or_else(|| "unknown".to_owned());
        if matches!(status.as_str(), "exited" | "dead") {
          return Err(IoError::interrupted(
            "CargoReadiness",
            &format!(
              "Cargo {:?} process {} exited before becoming ready",
              slot.container_name, slot.process.key
            ),
          ));
        }
        pending.push(format!("{} runtime={status}", slot.container_name));
        continue;
      }
      if slot.role == CargoReplicaProcessRole::Sandbox {
        continue;
      }
      let healthcheck = inspected
        .config
        .as_ref()
        .and_then(|config| config.healthcheck.as_ref());
      if !healthcheck_enabled(healthcheck) {
        continue;
      }
      any_healthcheck = true;
      let health = container_state
        .and_then(|state| state.health.as_ref())
        .and_then(|health| health.status.as_ref())
        .map(ToString::to_string)
        .unwrap_or_else(|| "starting".to_owned());
      match health.as_str() {
        "healthy" => {}
        "unhealthy" => {
          return Err(IoError::interrupted(
            "CargoReadiness",
            &format!(
              "Cargo {:?} process {} became unhealthy",
              slot.container_name, slot.process.key
            ),
          ));
        }
        _ => pending.push(format!("{} health={health}", slot.container_name)),
      }
    }
    if pending.is_empty() {
      return Ok(any_healthcheck);
    }
    if Instant::now() >= deadline {
      return Err(IoError::interrupted(
        "CargoReadiness",
        &format!(
          "Timed out waiting for essential Cargo processes: {}",
          pending.join(", ")
        ),
      ));
    }
    ntex::time::sleep(READY_POLL_INTERVAL).await;
  }
}

async fn refresh_slot_observations(
  slots: &mut [RuntimeSlot],
  state: &SystemState,
) -> IoResult<()> {
  for slot in slots {
    let inspected = inspect_process(&slot.process.key, state).await?;
    let data = serde_json::to_value(&inspected)
      .map_err(|error| error.map_err_context(|| "ObserveCargoProcess"))?;
    let updated_at = chrono::Utc::now().naive_utc();
    ProcessDb::update_pk(
      &slot.process.key,
      ProcessUpdateDb {
        data: Some(data),
        updated_at: Some(updated_at),
        ..Default::default()
      },
      &state.inner.pool,
    )
    .await?;
    slot.process.data = inspected;
    slot.process.updated_at = updated_at;
  }
  Ok(())
}

async fn ensure_existing_slot(
  opts: EnsureExistingSlotOpts<'_>,
) -> IoResult<RuntimeSlot> {
  let EnsureExistingSlotOpts {
    cargo,
    task,
    mapping,
    role,
    container_name,
    essential,
    config,
    state,
  } = opts;
  let process = match mapped_process(&mapping, state).await? {
    Some(process) => {
      let identity = process_identity(&process)?;
      if identity.replica_key != task.key
        || identity.ordinal != task.ordinal
        || identity.role != role
        || identity.container_name != container_name
      {
        return Err(IoError::other(
          "CargoRuntimeIdentity",
          &format!(
            "Cargo mapping {} does not match attached process {} identity",
            mapping.key, process.key
          ),
        ));
      }
      process
    }
    None => {
      let process =
        create_process(cargo, task, role, container_name, config, false, state)
          .await?;
      attach_process(&mapping, &process, state).await?;
      process
    }
  };
  Ok(RuntimeSlot {
    mapping,
    process,
    role,
    container_name: container_name.to_owned(),
    essential,
  })
}

async fn reconcile_replica(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  state: &SystemState,
) -> IoResult<ReplicaRuntime> {
  preflight_replica(cargo, task, state).await?;
  let gateway = internal_gateway(cargo, state).await?;
  let attempt_id = uuid::Uuid::new_v4().to_string();
  let mut slots = Vec::new();

  let sandbox_config = compile_sandbox_config(cargo, task, state).await?;
  let sandbox = if let Some(config) = sandbox_config {
    let mapping = ensure_mapping(
      task,
      CargoReplicaProcessRole::Sandbox,
      SANDBOX_LOGICAL_NAME,
      true,
      state,
    )
    .await?;
    let slot = ensure_existing_slot(EnsureExistingSlotOpts {
      cargo,
      task,
      mapping,
      role: CargoReplicaProcessRole::Sandbox,
      container_name: SANDBOX_LOGICAL_NAME,
      essential: true,
      config: &config,
      state,
    })
    .await?;
    start_process(&slot.process, state).await?;
    let id = slot.process.key.clone();
    slots.push(slot);
    Some(id)
  } else {
    None
  };

  reconcile_containers(
    cargo,
    task,
    sandbox.as_deref(),
    &attempt_id,
    &gateway,
    &mut slots,
    state,
  )
  .await?;

  for declared in &cargo.spec.containers {
    let config = compile_declared(CompileDeclaredOpts {
      cargo,
      task,
      declared,
      role: CargoReplicaProcessRole::App,
      sandbox_id: sandbox.as_deref(),
      attempt_id: &attempt_id,
      gateway: &gateway,
      state,
    })
    .await?;
    let mapping = ensure_mapping(
      task,
      CargoReplicaProcessRole::App,
      &declared.name,
      declared.essential,
      state,
    )
    .await?;
    let slot = ensure_existing_slot(EnsureExistingSlotOpts {
      cargo,
      task,
      mapping,
      role: CargoReplicaProcessRole::App,
      container_name: &declared.name,
      essential: declared.essential,
      config: &config,
      state,
    })
    .await?;
    start_process(&slot.process, state).await?;
    slots.push(slot);
  }
  wait_until_ready(&slots, state).await?;
  refresh_slot_observations(&mut slots, state).await?;
  Ok(ReplicaRuntime { slots })
}

fn restart_topology_error(cargo_key: &str, reason: &str) -> IoError {
  IoError::other(
    "CargoRestartTopology",
    &format!("Cargo {cargo_key:?} cannot be restarted: {reason}"),
  )
}

async fn preflight_cargo_restart(
  plan: &mut CargoRestartPlan,
  state: &SystemState,
) -> IoResult<()> {
  for slot in &mut plan.slots {
    let expected_identity = process_identity(&slot.process)?;
    let inspected = inspect_process(&slot.process.key, state).await?;
    let current_name = inspected
      .name
      .as_deref()
      .map(|name| name.trim_start_matches('/'))
      .filter(|name| !name.is_empty())
      .unwrap_or(&slot.process.name)
      .to_owned();
    if is_rollout_process_name(&current_name) {
      return Err(restart_topology_error(
        &slot.process.kind_key,
        &format!(
          "mapped {} process {} is currently named {current_name:?}",
          slot.role, slot.process.key
        ),
      ));
    }
    slot.process.name = current_name;
    slot.process.data = inspected;
    let identity = process_identity(&slot.process)?;
    if identity != expected_identity {
      return Err(restart_topology_error(
        &slot.process.kind_key,
        &format!(
          "mapped {} process {} changed runtime identity before restart",
          slot.role, slot.process.key
        ),
      ));
    }
    if slot.role == CargoReplicaProcessRole::Sandbox
      && !observed_container_running(&slot.process.data)
    {
      return Err(restart_topology_error(
        &slot.process.kind_key,
        &format!(
          "sandbox process {} is not running and will not be repaired by restart",
          slot.process.key
        ),
      ));
    }
  }
  Ok(())
}

async fn mark_cargo_restarting(
  cargo_key: &str,
  state: &SystemState,
) -> IoResult<()> {
  let current = ObjPsStatusDb::read_by_pk(cargo_key, &state.inner.pool).await?;
  ObjPsStatusDb::update_pk(
    cargo_key,
    ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(current.wanted),
      actual: Some(ObjPsStatusKind::Starting.to_string()),
      prev_actual: Some(current.actual),
      health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
    },
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

/// Docker-restart every mapped application while preserving the sandbox and
/// completed init containers, then publish success only after full essential
/// readiness is restored.
pub async fn restart(key: &str, state: &SystemState) -> IoResult<()> {
  let cargo = CargoDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let replicas = CargoReplicaDb::list_by_cargo(key, &state.inner.pool).await?;
  let mut mappings = Vec::new();
  for replica in &replicas {
    mappings.extend(
      CargoReplicaProcessDb::list_by_replica(replica.key, &state.inner.pool)
        .await?,
    );
  }
  let processes =
    ProcessDb::read_by_kind_key(key, None, &state.inner.pool).await?;
  let mut plan = plan_cargo_restart(
    &cargo,
    &replicas,
    &mappings,
    &processes,
    &state.inner.config.hostname,
  )?;
  preflight_cargo_restart(&mut plan, state).await?;
  mark_cargo_restarting(key, state).await?;

  for slot in plan
    .slots
    .iter()
    .filter(|slot| slot.role == CargoReplicaProcessRole::App)
  {
    state
      .inner
      .docker_api
      .restart_container(&slot.process.key, None)
      .await
      .map_err(|error| error.map_err_context(|| "RestartCargoProcess"))?;
  }
  let any_healthcheck = wait_until_ready(&plan.slots, state).await?;
  refresh_slot_observations(&mut plan.slots, state).await?;
  if !restart_required_slots_ready(&plan.slots) {
    return Err(IoError::interrupted(
      "CargoReadiness",
      &format!(
        "Cargo {key:?} changed readiness after its application containers were restarted"
      ),
    ));
  }
  let status = ObjPsStatusDb::read_by_pk(key, &state.inner.pool).await?;
  if status.actual != ObjPsStatusKind::Starting.to_string()
    || status.wanted != ObjPsStatusKind::Start.to_string()
  {
    return Err(IoError::interrupted(
      "CargoRestart",
      &format!(
        "Cargo {key:?} lifecycle changed to wanted={} actual={} while restart was waiting for readiness",
        status.wanted, status.actual
      ),
    ));
  }
  if any_healthcheck {
    ObjPsStatusDb::update_health_status(
      key,
      &ObjPsHealthStatusKind::Healthy,
      &state.inner.pool,
    )
    .await?;
  }
  ObjPsStatusDb::update_actual_status(
    key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  for action in promotion_actions(any_healthcheck) {
    state.emit_normal_native_action_sync(&cargo, action).await;
  }
  Ok(())
}

/// Start exactly the persisted replica identities assigned to this node.
pub async fn start(
  cargo: &Cargo,
  replicas: &[CargoReplicaTask],
  state: &SystemState,
) -> IoResult<()> {
  let mut replicas = replicas.to_vec();
  replicas.sort_by_key(|replica| replica.ordinal);
  validate_local_fixed_port_ownership(cargo, replicas.len())?;
  let processes =
    ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, None, &state.inner.pool)
      .await?;
  validate_observed_identities(
    &cargo.spec.cargo_key,
    &replicas,
    &processes,
    &state.inner.config.hostname,
  )?;
  for replica in &replicas {
    let persisted = CargoReplicaDb::get(replica.key, &state.inner.pool).await?;
    if persisted.cargo_key != cargo.spec.cargo_key
      || persisted.ordinal != replica.ordinal
      || persisted.node_name.as_deref() != Some(&state.inner.config.hostname)
    {
      return Err(IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo replica {} / ordinal {} is not assigned to node {:?}",
          replica.key, replica.ordinal, state.inner.config.hostname
        ),
      ));
    }
    reconcile_replica(cargo, replica, state).await?;
  }
  Ok(())
}

async fn active_slots(
  task: &CargoReplicaTask,
  state: &SystemState,
) -> IoResult<Vec<RuntimeSlot>> {
  let mappings =
    CargoReplicaProcessDb::list_by_replica(task.key, &state.inner.pool).await?;
  let mut slots = Vec::new();
  for mapping in mappings {
    let Some(process) = mapped_process(&mapping, state).await? else {
      continue;
    };
    let identity = process_identity(&process)?;
    slots.push(RuntimeSlot {
      role: mapping.role,
      container_name: mapping.container_name.clone(),
      essential: mapping.essential,
      mapping,
      process,
    });
    let slot = slots.last().expect("a runtime slot was just inserted");
    if identity.replica_key != task.key
      || identity.ordinal != task.ordinal
      || identity.role != slot.role
      || identity.container_name != slot.container_name
      || identity.essential != slot.essential
    {
      return Err(IoError::other(
        "CargoRuntimeIdentity",
        "Persisted Cargo mapping does not match its concrete process identity",
      ));
    }
  }
  Ok(slots)
}

async fn rename_old_slots(
  slots: &mut [RuntimeSlot],
  state: &SystemState,
) -> IoResult<()> {
  for slot in slots {
    if slot.process.name.starts_with("tmp-") {
      ProcessDb::update_pk(
        &slot.process.key,
        ProcessUpdateDb {
          name: Some(slot.process.name.clone()),
          updated_at: Some(chrono::Utc::now().naive_utc()),
          ..Default::default()
        },
        &state.inner.pool,
      )
      .await?;
      continue;
    }
    let name = format!("tmp-{}", slot.process.name);
    state
      .inner
      .docker_api
      .rename_container(
        &slot.process.key,
        RenameContainerOptions { name: &name },
      )
      .await
      .map_err(|error| error.map_err_context(|| "RenameCargoProcess"))?;
    slot.process.name = name.clone();
    ProcessDb::update_pk(
      &slot.process.key,
      ProcessUpdateDb {
        name: Some(name.clone()),
        updated_at: Some(chrono::Utc::now().naive_utc()),
        ..Default::default()
      },
      &state.inner.pool,
    )
    .await?;
  }
  Ok(())
}

async fn set_candidate_marker(
  slots: &mut [RuntimeSlot],
  pending: bool,
  state: &SystemState,
) -> IoResult<()> {
  for slot in slots {
    let (name, rename) = if pending {
      if slot.process.name.starts_with("candidate-") {
        (slot.process.name.clone(), false)
      } else {
        (format!("candidate-{}", slot.process.name), true)
      }
    } else {
      let Some(name) = slot.process.name.strip_prefix("candidate-") else {
        continue;
      };
      (name.to_owned(), true)
    };
    if rename {
      state
        .inner
        .docker_api
        .rename_container(
          &slot.process.key,
          RenameContainerOptions { name: &name },
        )
        .await
        .map_err(|error| error.map_err_context(|| "MarkCargoCandidate"))?;
      slot.process.name = name.clone();
    }
    ProcessDb::update_pk(
      &slot.process.key,
      ProcessUpdateDb {
        name: Some(name),
        updated_at: Some(chrono::Utc::now().naive_utc()),
        ..Default::default()
      },
      &state.inner.pool,
    )
    .await?;
  }
  Ok(())
}

async fn activate_candidate_slots(
  slots: &mut [RuntimeSlot],
  state: &SystemState,
) -> IoResult<()> {
  // The proxy requires the exact essential application set before selecting
  // an active attempt. Activate infrastructure and nonessential processes
  // first, then essential applications last so no fallible rename follows a
  // newly routable group.
  for essential_phase in [false, true] {
    for slot in slots.iter_mut().filter(|slot| {
      (slot.role == CargoReplicaProcessRole::App && slot.essential)
        == essential_phase
    }) {
      set_candidate_marker(std::slice::from_mut(slot), false, state).await?;
    }
  }
  Ok(())
}

async fn retain_old_slots(
  slots: &mut [RuntimeSlot],
  restart: bool,
  state: &SystemState,
) -> IoResult<()> {
  // The persisted declaration is already the candidate declaration. Keep the
  // old generation marked as retained so the proxy can validate it from its
  // observed labels instead of mistaking it for an incomplete active
  // generation from the new spec.
  rename_old_slots(slots, state).await?;
  if restart {
    for slot in slots
      .iter()
      .filter(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
    {
      start_process(&slot.process, state).await?;
    }
    for slot in slots
      .iter()
      .filter(|slot| slot.role == CargoReplicaProcessRole::App)
    {
      start_process(&slot.process, state).await?;
    }
  }
  Ok(())
}

async fn stop_slots(
  slots: &[RuntimeSlot],
  state: &SystemState,
) -> IoResult<()> {
  for slot in slots
    .iter()
    .filter(|slot| slot.role == CargoReplicaProcessRole::App)
  {
    stop_process(&slot.process, state).await?;
  }
  for slot in slots
    .iter()
    .filter(|slot| slot.role == CargoReplicaProcessRole::Sandbox)
  {
    stop_process(&slot.process, state).await?;
  }
  Ok(())
}

async fn delete_slots(
  slots: &[RuntimeSlot],
  state: &SystemState,
) -> IoResult<()> {
  for role in RUNTIME_DELETION_ORDER {
    for slot in slots.iter().filter(|slot| slot.role == role) {
      super::process::delete_instance(
        &slot.process.key,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
        state,
      )
      .await?;
    }
  }
  Ok(())
}

async fn create_candidate(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  state: &SystemState,
) -> IoResult<ReplicaRuntime> {
  let gateway = internal_gateway(cargo, state).await?;
  let attempt_id = uuid::Uuid::new_v4().to_string();
  let mut slots = Vec::new();
  let result = async {
    let sandbox_config = compile_sandbox_config(cargo, task, state).await?;
    let sandbox = if let Some(config) = sandbox_config {
      let mapping = ensure_mapping(
        task,
        CargoReplicaProcessRole::Sandbox,
        SANDBOX_LOGICAL_NAME,
        true,
        state,
      )
      .await?;
      let process = create_process(
        cargo,
        task,
        CargoReplicaProcessRole::Sandbox,
        SANDBOX_LOGICAL_NAME,
        &config,
        true,
        state,
      )
      .await?;
      let id = process.key.clone();
      slots.push(RuntimeSlot {
        mapping,
        process,
        role: CargoReplicaProcessRole::Sandbox,
        container_name: SANDBOX_LOGICAL_NAME.to_owned(),
        essential: true,
      });
      start_process(
        &slots
          .last()
          .expect("a sandbox candidate was just inserted")
          .process,
        state,
      )
      .await?;
      Some(id)
    } else {
      None
    };
    create_candidate_containers(
      cargo,
      task,
      sandbox.as_deref(),
      &attempt_id,
      &gateway,
      &mut slots,
      state,
    )
    .await?;
    for declared in &cargo.spec.containers {
      let config = compile_declared(CompileDeclaredOpts {
        cargo,
        task,
        declared,
        role: CargoReplicaProcessRole::App,
        sandbox_id: sandbox.as_deref(),
        attempt_id: &attempt_id,
        gateway: &gateway,
        state,
      })
      .await?;
      let mapping = ensure_mapping(
        task,
        CargoReplicaProcessRole::App,
        &declared.name,
        declared.essential,
        state,
      )
      .await?;
      let process = create_process(
        cargo,
        task,
        CargoReplicaProcessRole::App,
        &declared.name,
        &config,
        true,
        state,
      )
      .await?;
      slots.push(RuntimeSlot {
        mapping,
        process,
        role: CargoReplicaProcessRole::App,
        container_name: declared.name.clone(),
        essential: declared.essential,
      });
      start_process(
        &slots
          .last()
          .expect("an application candidate was just inserted")
          .process,
        state,
      )
      .await?;
    }
    wait_until_ready(&slots, state).await
  }
  .await;
  match result {
    Ok(_) => Ok(ReplicaRuntime { slots }),
    Err(error) => {
      let _ = delete_slots(&slots, state).await;
      Err(error)
    }
  }
}

async fn remove_stale_mappings(
  task: &CargoReplicaTask,
  candidate: &ReplicaRuntime,
  state: &SystemState,
) -> IoResult<()> {
  let desired = candidate
    .slots
    .iter()
    .map(|slot| (slot.role, slot.container_name.as_str()))
    .collect::<HashSet<_>>();
  let mappings =
    CargoReplicaProcessDb::list_by_replica(task.key, &state.inner.pool).await?;
  for mapping in mappings {
    if !desired.contains(&(mapping.role, mapping.container_name.as_str())) {
      CargoReplicaProcessDb::delete(mapping.key, &state.inner.pool).await?;
    }
  }
  Ok(())
}

async fn prepare_replica_update(
  cargo: &Cargo,
  task: &CargoReplicaTask,
  stop_first: bool,
  remove_first: bool,
  state: &SystemState,
) -> IoResult<PreparedReplicaUpdate> {
  preflight_replica(cargo, task, state).await?;
  let mut old = active_slots(task, state).await?;
  if let Err(error) = rename_old_slots(&mut old, state).await {
    let _ = retain_old_slots(&mut old, false, state).await;
    return Err(error);
  }
  if stop_first && let Err(error) = stop_slots(&old, state).await {
    let _ = retain_old_slots(&mut old, true, state).await;
    return Err(error);
  }
  if remove_first {
    if let Err(error) = delete_slots(&old, state).await {
      if let Err(restore_error) = retain_old_slots(&mut old, true, state).await
      {
        log::error!(
          "Unable to restore Cargo {} replica {} after fixed-port cleanup failure: {restore_error}",
          cargo.spec.cargo_key,
          task.key
        );
      }
      return Err(error);
    }
    // A fixed host-port rollout deliberately trades rollback for releasing the
    // old sandbox listener before the replacement sandbox is created.
    old.clear();
  }
  let mut candidate = match create_candidate(cargo, task, state).await {
    Ok(candidate) => candidate,
    Err(error) => {
      if let Err(restore_error) =
        retain_old_slots(&mut old, stop_first, state).await
      {
        log::error!(
          "Unable to restore Cargo {} replica {} after update failure: {restore_error}",
          cargo.spec.cargo_key,
          task.key
        );
      }
      return Err(error);
    }
  };
  if let Err(error) =
    refresh_slot_observations(&mut candidate.slots, state).await
  {
    let _ = delete_slots(&candidate.slots, state).await;
    let _ = retain_old_slots(&mut old, stop_first, state).await;
    return Err(error);
  }
  Ok(PreparedReplicaUpdate {
    task: task.clone(),
    old,
    candidate,
    stop_first,
  })
}

fn candidate_attachments(
  prepared: &[PreparedReplicaUpdate],
) -> Vec<(uuid::Uuid, Option<String>, bool)> {
  prepared
    .iter()
    .flat_map(|replica| &replica.candidate.slots)
    .map(|slot| {
      (
        slot.mapping.key,
        Some(slot.process.key.clone()),
        slot.essential,
      )
    })
    .collect()
}

fn previous_attachments(
  prepared: &[PreparedReplicaUpdate],
) -> Vec<(uuid::Uuid, Option<String>, bool)> {
  prepared
    .iter()
    .flat_map(|replica| &replica.candidate.slots)
    .map(|slot| {
      (
        slot.mapping.key,
        slot.mapping.process_key.clone(),
        slot.mapping.essential,
      )
    })
    .collect()
}

async fn rollback_prepared_updates(
  prepared: &mut [PreparedReplicaUpdate],
  state: &SystemState,
) {
  for replica in prepared.iter_mut().rev() {
    if let Err(error) =
      set_candidate_marker(&mut replica.candidate.slots, true, state).await
    {
      log::error!(
        "Unable to mark failed Cargo replica {} candidate as pending: {error}",
        replica.task.key
      );
    }
  }
  for replica in prepared.iter().rev() {
    if let Err(error) = delete_slots(&replica.candidate.slots, state).await {
      log::error!(
        "Unable to delete failed Cargo replica {} candidate: {error}",
        replica.task.key
      );
    }
  }
  for replica in prepared.iter_mut().rev() {
    if let Err(error) =
      retain_old_slots(&mut replica.old, replica.stop_first, state).await
    {
      log::error!(
        "Unable to restore Cargo replica {} after rollout failure: {error}",
        replica.task.key
      );
      continue;
    }
    if let Err(error) = refresh_slot_observations(&mut replica.old, state).await
    {
      log::error!(
        "Unable to refresh restored Cargo replica {} observations: {error}",
        replica.task.key
      );
    }
  }
}

async fn revalidate_prepared_candidates(
  prepared: &mut [PreparedReplicaUpdate],
  state: &SystemState,
) -> IoResult<bool> {
  let mut any_healthcheck = false;
  for replica in prepared {
    any_healthcheck |=
      wait_until_ready(&replica.candidate.slots, state).await?;
    refresh_slot_observations(&mut replica.candidate.slots, state).await?;
  }
  Ok(any_healthcheck)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RollbackCandidateCleanup {
  BeforeRetainedRestart,
  AfterRouteRefresh,
  PreserveForLiveness,
}

fn rollback_candidate_cleanup(
  stop_first: bool,
  retained_ready: bool,
) -> RollbackCandidateCleanup {
  if stop_first {
    RollbackCandidateCleanup::BeforeRetainedRestart
  } else if retained_ready {
    RollbackCandidateCleanup::AfterRouteRefresh
  } else {
    RollbackCandidateCleanup::PreserveForLiveness
  }
}

async fn restore_retained_updates(
  prepared: &mut [PreparedReplicaUpdate],
  state: &SystemState,
) -> bool {
  let mut all_ready = true;
  for replica in prepared {
    if let Err(error) =
      retain_old_slots(&mut replica.old, replica.stop_first, state).await
    {
      log::error!(
        "Unable to restore Cargo replica {} during rollback: {error}",
        replica.task.key
      );
      all_ready = false;
    }
    if let Err(error) = wait_until_ready(&replica.old, state).await {
      log::error!(
        "Restored Cargo replica {} did not become ready during rollback: {error}",
        replica.task.key
      );
      all_ready = false;
    }
    if let Err(error) = refresh_slot_observations(&mut replica.old, state).await
    {
      log::error!(
        "Unable to refresh restored Cargo replica {} observations: {error}",
        replica.task.key
      );
      all_ready = false;
    }
  }
  all_ready
}

async fn snapshot_prepared_candidate_readiness(
  cargo: &Cargo,
  prepared: &mut [PreparedReplicaUpdate],
  state: &SystemState,
) -> IoResult<bool> {
  if prepared.iter().any(|replica| {
    !required_candidate_topology_complete(
      cargo,
      replica
        .candidate
        .slots
        .iter()
        .map(|slot| (slot.role, slot.container_name.as_str(), slot.essential)),
    )
  }) {
    return Ok(false);
  }
  let targets = prepared
    .iter()
    .enumerate()
    .flat_map(|(replica_index, replica)| {
      replica
        .candidate
        .slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| {
          requires_steady_state_readiness(slot.role, slot.essential)
        })
        .map(move |(slot_index, slot)| {
          (replica_index, slot_index, slot.process.key.clone())
        })
    })
    .collect::<Vec<_>>();
  let observations = try_join_all(
    targets
      .iter()
      .map(|(_, _, process_key)| inspect_process(process_key, state)),
  )
  .await?;
  let mut ready = true;
  for ((replica_index, slot_index, _), inspected) in
    targets.into_iter().zip(observations)
  {
    let slot = &mut prepared[replica_index].candidate.slots[slot_index];
    let slot_ready = if slot.role == CargoReplicaProcessRole::Sandbox {
      observed_container_running(&inspected)
    } else {
      observed_container_ready(&inspected)
    };
    if !slot_ready {
      ready = false;
    }
    let data = serde_json::to_value(&inspected)
      .map_err(|error| error.map_err_context(|| "ObserveCargoProcess"))?;
    let updated_at = chrono::Utc::now().naive_utc();
    ProcessDb::update_pk(
      &slot.process.key,
      ProcessUpdateDb {
        data: Some(data),
        updated_at: Some(updated_at),
        ..Default::default()
      },
      &state.inner.pool,
    )
    .await?;
    slot.process.data = inspected;
    slot.process.updated_at = updated_at;
  }
  Ok(ready)
}

fn candidate_readiness_error(key: &str, phase: &str) -> IoError {
  IoError::interrupted(
    "CargoReadiness",
    &format!(
      "Cargo {key:?} candidate changed readiness during {phase}; retained generation was preserved"
    ),
  )
}

async fn rollback_committed_update(
  cargo: &Cargo,
  prepared: &mut [PreparedReplicaUpdate],
  previous_attachments: &[(uuid::Uuid, Option<String>, bool)],
  state: &SystemState,
) {
  for replica in prepared.iter_mut().rev() {
    if let Err(error) =
      set_candidate_marker(&mut replica.candidate.slots, true, state).await
    {
      log::error!(
        "Unable to hide Cargo replica {} candidate during rollback: {error}",
        replica.task.key
      );
    }
  }
  let attachments_restored = match CargoReplicaProcessDb::set_processes(
    previous_attachments.to_vec(),
    &state.inner.pool,
  )
  .await
  {
    Ok(_) => true,
    Err(error) => {
      log::error!(
        "Unable to restore Cargo {:?} process mappings during rollback: {error}",
        cargo.spec.cargo_key
      );
      false
    }
  };
  let stop_first = prepared.iter().any(|replica| replica.stop_first);
  if rollback_candidate_cleanup(stop_first, false)
    == RollbackCandidateCleanup::BeforeRetainedRestart
  {
    for replica in prepared.iter().rev() {
      if let Err(error) = delete_slots(&replica.candidate.slots, state).await {
        log::error!(
          "Unable to release Cargo replica {} candidate before restoring its retained fixed-port generation: {error}",
          replica.task.key
        );
      }
    }
  }
  let retained_ready =
    restore_retained_updates(prepared, state).await && attachments_restored;
  let candidate_cleanup =
    rollback_candidate_cleanup(stop_first, retained_ready);
  if let Err(error) = ObjPsStatusDb::update_health_status(
    &cargo.spec.cargo_key,
    &ObjPsHealthStatusKind::Unhealthy,
    &state.inner.pool,
  )
  .await
  {
    log::error!(
      "Unable to mark rolled-back Cargo {:?} unhealthy: {error}",
      cargo.spec.cargo_key
    );
  }
  if let Err(error) = ObjPsStatusDb::update_pk(
    &cargo.spec.cargo_key,
    ObjPsStatusUpdate {
      actual: Some(ObjPsStatusKind::Fail.to_string()),
      // Preserve an explicit failed-update marker even when readiness changed
      // after the provisional Start commit. Delayed reconciliation events must
      // not reinterpret the restored old topology as a successful candidate.
      prev_actual: Some(ObjPsStatusKind::Updating.to_string()),
      ..Default::default()
    },
    &state.inner.pool,
  )
  .await
  {
    log::error!(
      "Unable to mark rolled-back Cargo {:?} failed: {error}",
      cargo.spec.cargo_key
    );
  }
  if candidate_cleanup != RollbackCandidateCleanup::PreserveForLiveness {
    state
      .emit_normal_native_action_sync(cargo, NativeEventAction::Unhealthy)
      .await;
  }
  match candidate_cleanup {
    RollbackCandidateCleanup::BeforeRetainedRestart => {}
    RollbackCandidateCleanup::AfterRouteRefresh => {
      ntex::time::sleep(ROUTE_HANDOFF_GRACE).await;
      for replica in prepared.iter().rev() {
        if let Err(error) = delete_slots(&replica.candidate.slots, state).await
        {
          log::error!(
            "Unable to delete Cargo replica {} candidate after rollback route handoff: {error}",
            replica.task.key
          );
        }
      }
    }
    RollbackCandidateCleanup::PreserveForLiveness => {
      log::error!(
        "Cargo {:?} retained generation did not recover; hidden candidates and the existing route were preserved as a liveness fallback",
        cargo.spec.cargo_key
      );
    }
  }
}

/// Replace every locally assigned replica serially. Replicas without Cargo
/// port bindings keep their old serving sandbox until the candidate is ready.
/// Port-bound or host-network replicas are explicitly stopped first so Docker
/// cannot race the old listener for the same host socket.
pub async fn update(key: &str, state: &SystemState) -> IoResult<()> {
  let cargo = CargoDb::transform_read_by_pk(key, &state.inner.pool).await?;
  let replicas = CargoReplicaDb::list_by_cargo(key, &state.inner.pool).await?;
  if replicas.len() != cargo.spec.replicas
    || replicas.iter().any(|replica| replica.node_name.is_none())
  {
    return Err(IoError::other(
      "CargoUpdate",
      &format!(
        "Cargo {key:?} has {} durable replicas, {} desired replicas, or an unassigned replica",
        replicas.len(),
        cargo.spec.replicas
      ),
    ));
  }
  if let Some(remote) = replicas.iter().find(|replica| {
    replica.node_name.as_deref().is_some_and(|node| {
      node != state.inner.config.hostname && node != "init-test.nanocl.io"
    })
  }) {
    return Err(IoError::other(
      "CargoUpdate",
      &format!(
        "Cargo {key:?} replica {} is assigned to remote node {:?}; distributed rollout is not yet safe",
        remote.key, remote.node_name
      ),
    ));
  }
  let tasks = replicas
    .into_iter()
    .filter(|replica| replica.node_name.is_some())
    .map(|replica| CargoReplicaTask {
      key: replica.key,
      ordinal: replica.ordinal,
    })
    .collect::<Vec<_>>();
  validate_local_fixed_port_ownership(&cargo, tasks.len())?;
  for task in &tasks {
    preflight_replica(&cargo, task, state).await?;
  }
  let stop_first = requires_stop_before_replacement(&cargo);
  let remove_first = requires_removal_before_replacement(&cargo);
  let mut prepared = Vec::with_capacity(tasks.len());
  for task in &tasks {
    match prepare_replica_update(&cargo, task, stop_first, remove_first, state)
      .await
    {
      Ok(replica) => prepared.push(replica),
      Err(error) => {
        rollback_prepared_updates(&mut prepared, state).await;
        return Err(error);
      }
    }
  }
  let any_healthcheck =
    match revalidate_prepared_candidates(&mut prepared, state).await {
      Ok(any_healthcheck) => any_healthcheck,
      Err(error) => {
        rollback_prepared_updates(&mut prepared, state).await;
        return Err(error);
      }
    };
  match snapshot_prepared_candidate_readiness(&cargo, &mut prepared, state)
    .await
  {
    Ok(true) => {}
    Ok(false) => {
      rollback_prepared_updates(&mut prepared, state).await;
      return Err(candidate_readiness_error(key, "pre-promotion snapshot"));
    }
    Err(error) => {
      rollback_prepared_updates(&mut prepared, state).await;
      return Err(error);
    }
  }
  let previous_attachments = previous_attachments(&prepared);
  if let Err(error) = CargoReplicaProcessDb::set_processes(
    candidate_attachments(&prepared),
    &state.inner.pool,
  )
  .await
  {
    rollback_prepared_updates(&mut prepared, state).await;
    return Err(error);
  }
  // Keep the aggregate status Updating while candidates become visible. The
  // proxy therefore continues to select the retained generation, and Docker
  // health events cannot race the reconciler's final readiness decision.
  for replica in &mut prepared {
    if let Err(error) =
      activate_candidate_slots(&mut replica.candidate.slots, state).await
    {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(error);
    }
  }
  if let Err(error) = revalidate_prepared_candidates(&mut prepared, state).await
  {
    rollback_committed_update(
      &cargo,
      &mut prepared,
      &previous_attachments,
      state,
    )
    .await;
    return Err(error);
  }
  match snapshot_prepared_candidate_readiness(&cargo, &mut prepared, state)
    .await
  {
    Ok(true) => {}
    Ok(false) => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(candidate_readiness_error(key, "promotion snapshot"));
    }
    Err(error) => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(error);
    }
  }
  let status_result = async {
    if any_healthcheck {
      ObjPsStatusDb::update_health_status(
        key,
        &ObjPsHealthStatusKind::Healthy,
        &state.inner.pool,
      )
      .await?;
    }
    ObjPsStatusDb::update_actual_status(
      key,
      &ObjPsStatusKind::Start,
      &state.inner.pool,
    )
    .await
  }
  .await;
  if let Err(error) = status_result {
    rollback_committed_update(
      &cargo,
      &mut prepared,
      &previous_attachments,
      state,
    )
    .await;
    return Err(error);
  }
  match snapshot_prepared_candidate_readiness(&cargo, &mut prepared, state)
    .await
  {
    Ok(true) => {}
    Ok(false) => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(candidate_readiness_error(key, "status commit"));
    }
    Err(error) => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(error);
    }
  }
  for action in promotion_actions(any_healthcheck) {
    state.emit_normal_native_action_sync(&cargo, action).await;
  }
  ntex::time::sleep(ROUTE_HANDOFF_GRACE).await;
  let handoff_ready =
    snapshot_prepared_candidate_readiness(&cargo, &mut prepared, state).await;
  let status = ObjPsStatusDb::read_by_pk(key, &state.inner.pool).await;
  match (handoff_ready, status) {
    (Ok(true), Ok(status))
      if status.actual == ObjPsStatusKind::Start.to_string() => {}
    (Err(error), _) | (_, Err(error)) => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(error);
    }
    _ => {
      rollback_committed_update(
        &cargo,
        &mut prepared,
        &previous_attachments,
        state,
      )
      .await;
      return Err(candidate_readiness_error(key, "route handoff"));
    }
  }
  for replica in &prepared {
    if let Err(error) = delete_slots(&replica.old, state).await {
      log::error!(
        "Cargo {} replica {} is ready but its retained generation could not be removed: {error}",
        cargo.spec.cargo_key,
        replica.task.key
      );
      continue;
    }
    if let Err(error) =
      remove_stale_mappings(&replica.task, &replica.candidate, state).await
    {
      log::error!(
        "Cargo {} replica {} is ready but stale mappings could not be removed: {error}",
        cargo.spec.cargo_key,
        replica.task.key
      );
    }
  }
  Ok(())
}

/// Delete Cargo processes on this node (or all nodes when requested).
pub async fn delete_instances(
  key: &str,
  all: bool,
  state: &SystemState,
) -> IoResult<()> {
  let node_name = state.inner.config.hostname.clone();
  let mut filter = GenericFilter::new();
  if !all {
    filter = filter.r#where("node_name", GenericClause::Eq(node_name.clone()));
  }
  let processes =
    ProcessDb::read_by_kind_key(key, Some(filter), &state.inner.pool).await?;
  let mut remote_nodes = HashSet::new();
  let mut local = Vec::new();
  for process in processes {
    if process.node_name == node_name {
      local.push(process);
    } else {
      remote_nodes.insert(process.node_name);
    }
  }
  local.sort_by_key(|process| {
    let role = process_identity(process)
      .map(|identity| identity.role)
      .unwrap_or(CargoReplicaProcessRole::App);
    match role {
      CargoReplicaProcessRole::App => 0,
      CargoReplicaProcessRole::Init => 1,
      CargoReplicaProcessRole::Sandbox => 2,
    }
  });
  for process in local {
    super::process::delete_instance(
      &process.key,
      Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
      }),
      state,
    )
    .await?;
  }
  for remote_node in remote_nodes {
    let node = NodeDb::read_by_pk(&remote_node, &state.inner.pool).await?;
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: node.endpoint,
      ssl: None,
      version: Some(node.version),
    })?;
    client
      .delete_node_cargo(&DeleteNodeCargoParams {
        cargo_key: key.to_owned(),
      })
      .await?;
  }
  Ok(())
}

/// Delete Cargo runtime and desired state.
pub async fn delete(key: &str, state: &SystemState) -> IoResult<()> {
  delete_instances(key, true, state).await?;
  let cargo = CargoDb::transform_read_by_pk(key, &state.inner.pool).await?;
  CargoDb::clear_by_pk(key, &state.inner.pool).await?;
  state
    .emit_normal_native_action_sync(&cargo, NativeEventAction::Destroy)
    .await;
  Ok(())
}

#[cfg(test)]
mod tests {
  use bollard_next::models::{
    ContainerConfig, ContainerInspectResponse, ContainerState, Health,
    HealthConfig, HealthStatusEnum,
  };
  use nanocl_stubs::process::ProcessKind;

  use crate::utils::container::cargo_compiler::{
    LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA, LABEL_REPLICA_ORDINAL,
    LABEL_ROLE,
  };

  use super::*;

  pub(super) fn declared_container(
    name: &str,
    essential: bool,
  ) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: ImagePullPolicy::default(),
      container_config: DockerConfig::default(),
    }
  }

  pub(super) fn readiness_cargo(replicas: usize) -> Cargo {
    let mut cargo = Cargo::default();
    cargo.spec.replicas = replicas;
    cargo.spec.containers = vec![
      declared_container("api", true),
      declared_container("metrics", false),
    ];
    cargo
  }

  pub(super) fn readiness_process(
    replica: uuid::Uuid,
    process_name: &str,
    role: &str,
    logical_name: &str,
    essential: bool,
    running: bool,
    health: Option<HealthStatusEnum>,
  ) -> Process {
    let healthcheck = health.map(|_| HealthConfig {
      test: Some(vec!["CMD".to_owned(), "true".to_owned()]),
      ..Default::default()
    });
    Process {
      key: uuid::Uuid::new_v4().to_string(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      name: process_name.to_owned(),
      kind: ProcessKind::Cargo,
      node_name: "node-a".to_owned(),
      kind_key: "global.api".to_owned(),
      data: ContainerInspectResponse {
        config: Some(ContainerConfig {
          healthcheck,
          labels: Some(HashMap::from([
            (LABEL_REPLICA.to_owned(), replica.to_string()),
            (LABEL_REPLICA_ORDINAL.to_owned(), "0".to_owned()),
            (LABEL_ROLE.to_owned(), role.to_owned()),
            (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
            (LABEL_ESSENTIAL.to_owned(), essential.to_string()),
          ])),
          ..Default::default()
        }),
        state: Some(ContainerState {
          running: Some(running),
          health: health.map(|status| Health {
            status: Some(status),
            ..Default::default()
          }),
          ..Default::default()
        }),
        ..Default::default()
      },
    }
  }

  pub(super) fn ready_replica_processes(replica: uuid::Uuid) -> Vec<Process> {
    vec![
      readiness_process(
        replica,
        "global.api-sandbox.c",
        "sandbox",
        SANDBOX_LOGICAL_NAME,
        true,
        true,
        None,
      ),
      readiness_process(
        replica,
        "global.api-api.c",
        "app",
        "api",
        true,
        true,
        None,
      ),
    ]
  }

  #[test]
  fn declaration_positions_are_zero_based_within_each_container_role() {
    let mut cargo = Cargo::default();
    cargo.spec.cargo_key = "global.api".to_owned();
    cargo.spec.init_containers = vec![
      declared_container("prepare", true),
      declared_container("migrate", true),
    ];
    cargo.spec.containers = vec![
      declared_container("api", true),
      declared_container("worker", true),
    ];

    assert_eq!(
      declared_container_position(
        &cargo,
        &cargo.spec.init_containers[1],
        CargoReplicaProcessRole::Init,
      )
      .unwrap(),
      1
    );
    assert_eq!(
      declared_container_position(
        &cargo,
        &cargo.spec.containers[1],
        CargoReplicaProcessRole::App,
      )
      .unwrap(),
      1
    );
    assert!(
      declared_container_position(
        &cargo,
        &cargo.spec.containers[0],
        CargoReplicaProcessRole::Sandbox,
      )
      .is_err()
    );
  }

  #[test]
  fn runtime_cleanup_orders_applications_before_sandbox() {
    assert_eq!(
      RUNTIME_DELETION_ORDER,
      [
        CargoReplicaProcessRole::App,
        CargoReplicaProcessRole::Init,
        CargoReplicaProcessRole::Sandbox,
      ]
    );
  }

  #[test]
  fn rollback_cleanup_preserves_a_serving_candidate_until_old_is_safe() {
    assert_eq!(
      rollback_candidate_cleanup(true, false),
      RollbackCandidateCleanup::BeforeRetainedRestart
    );
    assert_eq!(
      rollback_candidate_cleanup(false, true),
      RollbackCandidateCleanup::AfterRouteRefresh
    );
    assert_eq!(
      rollback_candidate_cleanup(false, false),
      RollbackCandidateCleanup::PreserveForLiveness
    );
  }
}
