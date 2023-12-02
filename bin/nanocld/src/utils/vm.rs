use std::collections::HashMap;

use diesel::ExpressionMethods;
use ntex::http;

use bollard_next::Docker;
use bollard_next::service::{HostConfig, DeviceMapping, ContainerSummary};
use bollard_next::container::{
  ListContainersOptions, StopContainerOptions, RemoveContainerOptions,
};

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::system::EventAction;
use nanocl_stubs::vm_spec::{VmSpecPartial, VmSpecUpdate};
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};

use crate::utils;
use crate::models::{
  Pool, VmImageDb, DaemonState, ProcessDb, NamespaceDb, Repository, VmDb,
  VmSpecDb, FromSpec, ProcessKind,
};

/// Stop a VM by his model
pub(crate) async fn stop(vm: &Vm, state: &DaemonState) -> HttpResult<()> {
  let container_name = format!("{}.v", vm.spec.vm_key);
  state
    .docker_api
    .stop_container(&container_name, None::<StopContainerOptions>)
    .await
    .map_err(|e| HttpError {
      msg: format!("Unable to stop container got error : {e}"),
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  state
    .event_emitter
    .spawn_emit_to_event(vm, EventAction::Stopped);
  Ok(())
}

/// Stop a VM by his key
pub(crate) async fn stop_by_key(
  vm_key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let vm = VmDb::inspect_by_pk(vm_key, &state.pool).await?;
  stop(&vm, state).await
}

/// Get detailed information about a VM by his key
pub(crate) async fn inspect_by_key(
  vm_key: &str,
  state: &DaemonState,
) -> HttpResult<VmInspect> {
  let vm = VmDb::inspect_by_pk(vm_key, &state.pool).await?;
  let containers =
    list_instances_by_key(&vm.spec.vm_key, &state.docker_api).await?;
  let mut running_instances = 0;
  for container in &containers {
    if container.state == Some("running".into()) {
      running_instances += 1;
    }
  }
  Ok(VmInspect {
    created_at: vm.created_at,
    namespace_name: vm.namespace_name,
    spec: vm.spec,
    instance_total: containers.len(),
    instance_running: running_instances,
    instances: containers,
  })
}

/// List VM instances by his key
pub(crate) async fn list_instances_by_key(
  vm_key: &str,
  docker_api: &Docker,
) -> HttpResult<Vec<ContainerSummary>> {
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

/// Delete a VM by his key
pub(crate) async fn delete_by_key(
  vm_key: &str,
  force: bool,
  state: &DaemonState,
) -> HttpResult<()> {
  let vm = VmDb::inspect_by_pk(vm_key, &state.pool).await?;
  let options = bollard_next::container::RemoveContainerOptions {
    force,
    ..Default::default()
  };
  let container_name = format!("{}.v", vm_key);
  let _ = state
    .docker_api
    .remove_container(&container_name, Some(options))
    .await;
  VmDb::delete_by_pk(vm_key, &state.pool).await??;
  VmSpecDb::delete_by(
    crate::schema::vm_specs::dsl::vm_key.eq(vm.spec.vm_key.clone()),
    &state.pool,
  )
  .await??;
  utils::vm_image::delete_by_name(&vm.spec.disk.image, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&vm, EventAction::Deleted);
  Ok(())
}

/// List VMs by namespace
pub(crate) async fn list_by_namespace(
  nsp: &str,
  pool: &Pool,
) -> HttpResult<Vec<VmSummary>> {
  let namespace = NamespaceDb::find_by_pk(nsp, pool).await??;
  let vmes = VmDb::find_by_namespace(&namespace.name, pool).await?;
  let mut vm_summaries = Vec::new();
  for vm in vmes {
    let spec = VmSpecDb::find_by_pk(&vm.spec.key, pool)
      .await??
      .try_to_spec()?;
    let instances = ProcessDb::find_by_kind_key(&vm.spec.vm_key, pool).await?;
    let mut running_instances = 0;
    for instance in &instances {
      if instance
        .data
        .state
        .clone()
        .unwrap_or_default()
        .running
        .unwrap_or_default()
      {
        running_instances += 1;
      }
    }
    vm_summaries.push(VmSummary {
      created_at: vm.created_at,
      namespace_name: vm.namespace_name,
      instances: instances.len(),
      running_instances,
      spec: spec.clone(),
    });
  }
  Ok(vm_summaries)
}

/// Create a VM instance from a VM image
pub(crate) async fn create_instance(
  vm: &Vm,
  image: &VmImageDb,
  disable_keygen: bool,
  state: &DaemonState,
) -> HttpResult<()> {
  let mut labels: HashMap<String, String> = HashMap::new();
  let vmimagespath = format!("{}/vms/images", state.config.state_dir);
  labels.insert("io.nanocl.v".to_owned(), vm.spec.vm_key.clone());
  labels.insert("io.nanocl.n".to_owned(), vm.namespace_name.clone());
  let mut args: Vec<String> =
    vec!["-hda".into(), image.path.clone(), "--nographic".into()];
  let host_config = vm.spec.host_config.clone();
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
    .spec
    .host_config
    .net_iface
    .clone()
    .unwrap_or("ens3".into());
  let link_net_iface = vm
    .spec
    .host_config
    .link_net_iface
    .clone()
    .unwrap_or("eth0".into());
  envs.push(format!("DEFAULT_INTERFACE={link_net_iface}"));
  envs.push(format!("FROM_NETWORK={net_iface}"));
  envs.push(format!("DELETE_SSH_KEY={disable_keygen}"));
  if let Some(user) = &vm.spec.user {
    envs.push(format!("USER={user}"));
  }
  if let Some(password) = &vm.spec.password {
    envs.push(format!("PASSWORD={password}"));
  }
  if let Some(ssh_key) = &vm.spec.ssh_key {
    envs.push(format!("SSH_KEY={ssh_key}"));
  }
  let image = match &vm.spec.host_config.runtime {
    Some(runtime) => runtime.to_owned(),
    None => "ghcr.io/nxthat/nanocl-qemu:8.0.2.0".into(),
  };
  let spec = bollard_next::container::Config {
    image: Some(image),
    tty: Some(true),
    hostname: vm.spec.hostname.clone(),
    env: Some(envs),
    labels: Some(labels),
    cmd: Some(args),
    attach_stderr: Some(true),
    attach_stdin: Some(true),
    attach_stdout: Some(true),
    open_stdin: Some(true),
    host_config: Some(HostConfig {
      network_mode: Some(
        vm.spec
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
  let name = format!("{}.v", &vm.spec.vm_key);
  utils::process::create(&name, "vm", &vm.spec.vm_key, spec, state).await?;
  Ok(())
}

/// Create a VM from a `VmSpecPartial` in the given namespace
pub(crate) async fn create(
  vm: &VmSpecPartial,
  namespace: &str,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Vm> {
  log::debug!(
    "Creating VM {} in namespace {} with version: {version}",
    vm.name,
    namespace
  );
  let vm_key = utils::key::gen_key(namespace, &vm.name);
  let mut vm = vm.clone();
  if VmDb::find_by_pk(&vm_key, &state.pool).await?.is_ok() {
    return Err(HttpError {
      status: http::StatusCode::CONFLICT,
      msg: format!(
        "VM with name {} already exists in namespace {namespace}",
        vm.name
      ),
    });
  }
  let image = VmImageDb::find_by_pk(&vm.disk.image, &state.pool).await??;
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
  let vm = VmDb::create_from_spec(namespace, &vm, version, &state.pool).await?;
  create_instance(&vm, &image, true, state).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&vm, EventAction::Created);
  Ok(vm)
}

/// Patch a VM specification from a `VmSpecUpdate` in the given namespace.
/// This will merge the new specification with the old one.
pub(crate) async fn patch(
  vm_key: &str,
  spec: &VmSpecUpdate,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Vm> {
  let vm = VmDb::find_by_pk(vm_key, &state.pool).await??;
  let old_spec = VmSpecDb::find_by_pk(&vm.spec_key, &state.pool)
    .await??
    .try_to_spec()?;
  let vm_partial = VmSpecPartial {
    name: spec.name.to_owned().unwrap_or(vm.name.clone()),
    disk: old_spec.disk,
    host_config: Some(
      spec.host_config.to_owned().unwrap_or(old_spec.host_config),
    ),
    hostname: if spec.hostname.is_some() {
      spec.hostname.clone()
    } else {
      old_spec.hostname
    },
    user: if spec.user.is_some() {
      spec.user.clone()
    } else {
      old_spec.user
    },
    password: if spec.password.is_some() {
      spec.password.clone()
    } else {
      old_spec.password
    },
    ssh_key: if spec.ssh_key.is_some() {
      spec.ssh_key.clone()
    } else {
      old_spec.ssh_key
    },
    mac_address: old_spec.mac_address,
    labels: if spec.labels.is_some() {
      spec.labels.clone()
    } else {
      old_spec.labels
    },
    metadata: if spec.metadata.is_some() {
      spec.metadata.clone()
    } else {
      old_spec.metadata
    },
  };
  put(vm_key, &vm_partial, version, state).await
}

/// Put a VM specification from a `VmSpecPartial` in the given namespace.
/// This will replace the old specification with the new one.
pub(crate) async fn put(
  vm_key: &str,
  vm_partial: &VmSpecPartial,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Vm> {
  let vm = VmDb::inspect_by_pk(vm_key, &state.pool).await?;
  let container_name = format!("{}.v", &vm.spec.vm_key);
  stop(&vm, state).await?;
  state
    .docker_api
    .remove_container(&container_name, None::<RemoveContainerOptions>)
    .await?;
  let vm =
    VmDb::update_from_spec(&vm.spec.vm_key, vm_partial, version, &state.pool)
      .await?;
  let image = VmImageDb::find_by_pk(&vm.spec.disk.image, &state.pool).await??;
  create_instance(&vm, &image, false, state).await?;
  utils::process::start_by_kind(&ProcessKind::Vm, &vm.spec.vm_key, state)
    .await?;
  state
    .event_emitter
    .spawn_emit_to_event(&vm, EventAction::Patched);
  Ok(vm)
}
