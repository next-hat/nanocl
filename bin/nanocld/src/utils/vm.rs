use std::collections::HashMap;

use ntex::{http, rt};

use bollard_next::Docker;
use bollard_next::service::{HostConfig, DeviceMapping, ContainerSummary};
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  StopContainerOptions, RemoveContainerOptions,
};

use nanocl_stubs::system::Event;
use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{Pool, VmImageDbModel, DaemonState};

/// ## Start by key
///
/// Start a VM by his key
///
/// ## Arguments
///
/// - [vm_key](str) - The vm key
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The vm has been started
///   - [Err](HttpError) - The vm has not been started
///
pub async fn start_by_key(
  vm_key: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let container_name = format!("{}.v", vm_key);
  state
    .docker_api
    .start_container(&container_name, None::<StartContainerOptions<String>>)
    .await
    .map_err(|e| HttpError {
      msg: format!("Unable to start container got error : {e}"),
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  let vm = repositories::vm::inspect_by_key(vm_key, &state.pool).await?;
  let event_emitter = state.event_emitter.clone();
  rt::spawn(async move {
    let _ = event_emitter.emit(Event::VmRunned(Box::new(vm))).await;
  });
  Ok(())
}

/// ## Stop
///
/// Stop a VM by his model
///
/// ## Arguments
///
/// - [vm](VmDbModel) - The vm model
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The vm has been stopped
///   - [Err](HttpError) - The vm has not been stopped
///
pub async fn stop(vm: &Vm, state: &DaemonState) -> Result<(), HttpError> {
  let container_name = format!("{}.v", vm.key);
  state
    .docker_api
    .stop_container(&container_name, None::<StopContainerOptions>)
    .await
    .map_err(|e| HttpError {
      msg: format!("Unable to stop container got error : {e}"),
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  let vm_ptr = vm.clone();
  let event_emitter = state.event_emitter.clone();
  rt::spawn(async move {
    let _ = event_emitter.emit(Event::VmStopped(Box::new(vm_ptr))).await;
  });
  Ok(())
}

/// ## Stop by key
///
/// Stop a VM by his key
///
/// ## Arguments
///
/// - [vm_key](str) - The vm key
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The vm has been stopped
///   - [Err](HttpError) - The vm has not been stopped
///
pub async fn stop_by_key(
  vm_key: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, &state.pool).await?;

  stop(&vm, state).await
}

/// ## Inspect by key
///
/// Inspect a VM by his key
///
/// ## Arguments
///
/// - [vm_key](str) - The vm key
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](VmInspect) - The vm has been inspected
///   - [Err](HttpError) - The vm has not been inspected
///
pub async fn inspect_by_key(
  vm_key: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<VmInspect, HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, pool).await?;
  let containers = list_instances_by_key(&vm.key, docker_api).await?;
  let mut running_instances = 0;
  for container in &containers {
    if container.state == Some("running".into()) {
      running_instances += 1;
    }
  }
  Ok(VmInspect {
    key: vm.key,
    name: vm.name,
    config_key: vm.config_key,
    namespace_name: vm.namespace_name,
    config: vm.config,
    instance_total: containers.len(),
    instance_running: running_instances,
    instances: containers,
  })
}

/// ## List instances by key
///
/// List VM instances by his key
///
/// ## Arguments
///
/// - [vm_key](str) - The vm key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The list of instances
///   - [Err](HttpError) - The list of instances has not been retrieved
///
pub async fn list_instances_by_key(
  vm_key: &str,
  docker_api: &Docker,
) -> Result<Vec<ContainerSummary>, HttpError> {
  let label = format!("io.nanocl.v={vm_key}");
  let mut filters: HashMap<&str, Vec<&str>> = HashMap::new();
  filters.insert("label", vec![&label]);
  let options = Some(ListContainersOptions {
    all: true,
    filters,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await?;
  Ok(containers)
}

/// ## Delete by key
///
/// Delete a VM by his key
///
/// ## Arguments
///
/// - [vm_key](str) - The vm key
/// - [force](bool) - Force the deletion
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The vm has been deleted
///   - [Err](HttpError) - The vm has not been deleted
///
pub async fn delete_by_key(
  vm_key: &str,
  force: bool,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, &state.pool).await?;
  let options = bollard_next::container::RemoveContainerOptions {
    force,
    ..Default::default()
  };
  let container_name = format!("{}.v", vm_key);
  let _ = state
    .docker_api
    .remove_container(&container_name, Some(options))
    .await;
  repositories::vm::delete_by_key(vm_key, &state.pool).await?;
  repositories::vm_config::delete_by_vm_key(&vm.key, &state.pool).await?;
  utils::vm_image::delete_by_name(&vm.config.disk.image, &state.pool).await?;
  let event_emitter = state.event_emitter.clone();
  let vm_ptr = vm.clone();
  rt::spawn(async move {
    let _ = event_emitter.emit(Event::VmDeleted(Box::new(vm_ptr))).await;
  });
  Ok(())
}

/// ## List by namespace
///
/// List VMs by namespace
///
/// ## Arguments
///
/// - [nsp](str) - The namespace name
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<VmSummary>) - The list of VMs
///   - [Err](HttpError) - The list of VMs has not been retrieved
///
pub async fn list_by_namespace(
  nsp: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<Vec<VmSummary>, HttpError> {
  let namespace = repositories::namespace::find_by_name(nsp, pool).await?;
  let vmes = repositories::vm::find_by_namespace(&namespace, pool).await?;
  let mut vm_summaries = Vec::new();
  for vm in vmes {
    let config =
      repositories::vm_config::find_by_key(&vm.config_key, pool).await?;
    let containers = list_instances_by_key(&vm.key, docker_api).await?;
    let mut running_instances = 0;
    for container in containers.clone() {
      if container.state == Some("running".into()) {
        running_instances += 1;
      }
    }
    vm_summaries.push(VmSummary {
      key: vm.key,
      created_at: vm.created_at,
      updated_at: config.created_at,
      name: vm.name,
      namespace_name: vm.namespace_name,
      config: config.to_owned(),
      instances: containers.len(),
      running_instances,
      config_key: config.key,
    });
  }
  Ok(vm_summaries)
}

/// ## Create instance
///
/// Create a VM instance from a VM image
///
/// ## Arguments
///
/// - [vm](Vm) - The VM
/// - [image](VmImageDbModel) - The VM image
/// - [disable_keygen](bool) - Disable SSH key generation
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The VM instance has been created
///   - [Err](HttpError) - The VM instance has not been created
///
pub async fn create_instance(
  vm: &Vm,
  image: &VmImageDbModel,
  disable_keygen: bool,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let mut labels: HashMap<String, String> = HashMap::new();
  let vmimagespath = format!("{}/vms/images", state.config.state_dir);
  labels.insert("io.nanocl".into(), "enabled".into());
  labels.insert("io.nanocl.v".into(), vm.key.clone());
  labels.insert("io.nanocl.vnsp".into(), vm.namespace_name.clone());
  let mut args: Vec<String> =
    vec!["-hda".into(), image.path.clone(), "--nographic".into()];
  let host_config = vm.config.host_config.clone();
  let kvm = host_config.kvm.unwrap_or_default();
  let mut devices = vec![DeviceMapping {
    path_on_host: Some("/dev/net/tun".into()),
    path_in_container: Some("/dev/net/tun".into()),
    cgroup_permissions: Some("rwm".into()),
  }];
  if kvm {
    args.push("-accel".into());
    args.push("kvm".into());
    devices.push(DeviceMapping {
      path_on_host: Some("/dev/kvm".into()),
      path_in_container: Some("/dev/kvm".into()),
      cgroup_permissions: Some("rwm".into()),
    });
    log::debug!("KVM enabled /dev/kvm mapped");
  }
  let cpu = host_config.cpu;
  let cpu = if cpu > 0 { cpu.to_string() } else { "1".into() };
  let cpu = cpu.clone();
  args.push("-smp".into());
  args.push(cpu.clone());
  let memory = host_config.memory;
  let memory = if memory > 0 {
    format!("{memory}M")
  } else {
    "512M".into()
  };
  args.push("-m".into());
  args.push(memory);
  let mut envs: Vec<String> = Vec::new();
  let net_iface = vm
    .config
    .host_config
    .net_iface
    .clone()
    .unwrap_or("ens3".into());
  let link_net_iface = vm
    .config
    .host_config
    .link_net_iface
    .clone()
    .unwrap_or("eth0".into());
  envs.push(format!("DEFAULT_INTERFACE={link_net_iface}"));
  envs.push(format!("FROM_NETWORK={net_iface}"));
  envs.push(format!("DELETE_SSH_KEY={disable_keygen}"));
  if let Some(user) = &vm.config.user {
    envs.push(format!("USER={user}"));
  }
  if let Some(password) = &vm.config.password {
    envs.push(format!("PASSWORD={password}"));
  }
  if let Some(ssh_key) = &vm.config.ssh_key {
    envs.push(format!("SSH_KEY={ssh_key}"));
  }
  let image = match &vm.config.host_config.runtime {
    Some(runtime) => runtime.to_owned(),
    None => "ghcr.io/nxthat/nanocl-qemu:8.0.2.0".into(),
  };
  let config = bollard_next::container::Config {
    image: Some(image),
    tty: Some(true),
    hostname: vm.config.hostname.clone(),
    env: Some(envs),
    labels: Some(labels),
    cmd: Some(args),
    attach_stderr: Some(true),
    attach_stdin: Some(true),
    attach_stdout: Some(true),
    open_stdin: Some(true),
    host_config: Some(HostConfig {
      network_mode: Some(
        vm.config
          .host_config
          .runtime_network
          .clone()
          .unwrap_or(vm.namespace_name.to_owned()),
      ),
      binds: Some(vec![format!("{vmimagespath}:{vmimagespath}")]),
      devices: Some(devices),
      cap_add: Some(vec!["NET_ADMIN".into()]),
      ..Default::default()
    }),
    ..Default::default()
  };
  let options = Some(CreateContainerOptions {
    name: format!("{}.v", &vm.key),
    ..Default::default()
  });
  state.docker_api.create_container(options, config).await?;
  let event_emitter = state.event_emitter.clone();
  let vm_ptr = vm.clone();
  rt::spawn(async move {
    let _ = event_emitter.emit(Event::VmCreated(Box::new(vm_ptr))).await;
  });
  Ok(())
}

/// ## Create
///
/// Create a VM from a `VmConfigPartial` in the given namespace
///
/// ## Arguments
///
/// - [vm](VmConfigPartial) - The VM configuration
/// - [namespace](str) - The namespace
/// - [version](str) - The version
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vm) - The VM has been created
///   - [Err](HttpError) - The VM has not been created
///
pub async fn create(
  vm: &VmConfigPartial,
  namespace: &str,
  version: &str,
  state: &DaemonState,
) -> Result<Vm, HttpError> {
  log::debug!(
    "Creating VM {} in namespace {} with version: {version}",
    vm.name,
    namespace
  );
  let vm_key = utils::key::gen_key(namespace, &vm.name);
  let mut vm = vm.clone();
  if repositories::vm::find_by_key(&vm_key, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: http::StatusCode::CONFLICT,
      msg: format!(
        "VM with name {} already exists in namespace {namespace}",
        vm.name
      ),
    });
  }
  let image =
    repositories::vm_image::find_by_name(&vm.disk.image, &state.pool).await?;
  if image.kind.as_str() != "Base" {
    return Err(HttpError {
      msg: format!("Image {} is not a base image please convert the snapshot into a base image first", &vm.disk.image),
      status: http::StatusCode::BAD_REQUEST,
    });
  }
  let snapname = format!("{}.{vm_key}", &image.name);
  let size = vm.disk.size.unwrap_or(20);
  let image =
    utils::vm_image::create_snap(&snapname, size, &image, state).await?;
  // Use the snapshot image
  vm.disk.image = image.name.clone();
  vm.disk.size = Some(size);
  let vm =
    repositories::vm::create(namespace, &vm, version, &state.pool).await?;
  create_instance(&vm, &image, true, state).await?;
  Ok(vm)
}

/// ## Patch
///
/// Patch a VM configuration from a `VmConfigUpdate` in the given namespace.
/// This will merge the new configuration with the old one.
///
/// ## Arguments
///
/// - [vm_key](str) - The VM key
/// - [config](VmConfigUpdate) - The VM configuration
/// - [version](str) - The version
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vm) - The VM has been patched
///   - [Err](HttpError) - The VM has not been patched
///
pub async fn patch(
  vm_key: &str,
  config: &VmConfigUpdate,
  version: &str,
  state: &DaemonState,
) -> Result<Vm, HttpError> {
  let vm = repositories::vm::find_by_key(vm_key, &state.pool).await?;
  let old_config =
    repositories::vm_config::find_by_key(&vm.config_key, &state.pool).await?;
  let vm_partial = VmConfigPartial {
    name: config.name.to_owned().unwrap_or(vm.name.clone()),
    disk: old_config.disk,
    host_config: Some(
      config
        .host_config
        .to_owned()
        .unwrap_or(old_config.host_config),
    ),
    hostname: if config.hostname.is_some() {
      config.hostname.clone()
    } else {
      old_config.hostname
    },
    user: if config.user.is_some() {
      config.user.clone()
    } else {
      old_config.user
    },
    password: if config.password.is_some() {
      config.password.clone()
    } else {
      old_config.password
    },
    ssh_key: if config.ssh_key.is_some() {
      config.ssh_key.clone()
    } else {
      old_config.ssh_key
    },
    mac_address: old_config.mac_address,
    labels: if config.labels.is_some() {
      config.labels.clone()
    } else {
      old_config.labels
    },
    metadata: if config.metadata.is_some() {
      config.metadata.clone()
    } else {
      old_config.metadata
    },
  };
  put(vm_key, &vm_partial, version, state).await
}

/// ## Put
///
/// Put a VM configuration from a `VmConfigPartial` in the given namespace.
/// This will replace the old configuration with the new one.
///
/// ## Arguments
///
/// - [vm_key](str) - The VM key
/// - [vm_partial](VmConfigPartial) - The VM configuration
/// - [version](str) - The version
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vm) - The VM has been put
///   - [Err](HttpError) - The VM has not been put
///
pub async fn put(
  vm_key: &str,
  vm_partial: &VmConfigPartial,
  version: &str,
  state: &DaemonState,
) -> Result<Vm, HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, &state.pool).await?;
  let container_name = format!("{}.v", &vm.key);
  stop(&vm, state).await?;
  state
    .docker_api
    .remove_container(&container_name, None::<RemoveContainerOptions>)
    .await?;
  let vm =
    repositories::vm::update_by_key(&vm.key, vm_partial, version, &state.pool)
      .await?;
  let image =
    repositories::vm_image::find_by_name(&vm.config.disk.image, &state.pool)
      .await?;
  create_instance(&vm, &image, false, state).await?;
  start_by_key(&vm.key, state).await?;
  let event_emitter = state.event_emitter.clone();
  let vm_ptr = vm.clone();
  rt::spawn(async move {
    let _ = event_emitter.emit(Event::VmPatched(Box::new(vm_ptr))).await;
  });
  Ok(vm)
}
