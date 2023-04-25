use std::collections::HashMap;

use ntex::http::StatusCode;

use bollard_next::Docker;
use bollard_next::service::{HostConfig, DeviceMapping, ContainerSummary};
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  StopContainerOptions, RemoveContainerOptions,
};

use nanocl_stubs::vm_config::{VmConfigPartial, VmConfigUpdate};
use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};

use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;
use crate::models::{Pool, VmDbModel, VmImageDbModel, DaemonState};

pub async fn start(vm_key: &str, docker_api: &Docker) -> Result<(), HttpError> {
  let container_name = format!("{}.v", vm_key);
  docker_api
    .start_container(&container_name, None::<StartContainerOptions<String>>)
    .await
    .map_err(|e| HttpError {
      msg: format!("Unable to start container got error : {e}"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;

  Ok(())
}

/// Stop a VM by his model
pub async fn stop(
  vm: &VmDbModel,
  docker_api: &Docker,
) -> Result<(), HttpError> {
  let container_name = format!("{}.v", vm.key);
  docker_api
    .stop_container(&container_name, None::<StopContainerOptions>)
    .await
    .map_err(|e| HttpError {
      msg: format!("Unable to stop container got error : {e}"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  Ok(())
}

/// Stop a VM by key
pub async fn stop_by_key(
  vm_key: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<(), HttpError> {
  let vm = repositories::vm::find_by_key(vm_key, pool).await?;

  stop(&vm, docker_api).await
}

pub async fn inspect(
  vm_key: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<VmInspect, HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, pool).await?;
  let containers = list_instance(&vm.key, docker_api).await?;

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

pub async fn list_instance(
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

pub async fn delete(
  vm_key: &str,
  force: bool,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<(), HttpError> {
  let vm = repositories::vm::inspect_by_key(vm_key, pool).await?;

  let options = bollard_next::container::RemoveContainerOptions {
    force,
    ..Default::default()
  };

  let container_name = format!("{}.v", vm_key);
  let _ = docker_api
    .remove_container(&container_name, Some(options))
    .await;

  repositories::vm::delete_by_key(vm_key, pool).await?;
  repositories::vm_config::delete_by_vm_key(&vm.key, pool).await?;
  utils::vm_image::delete(&vm.config.disk.image, pool).await?;

  Ok(())
}

pub async fn list(
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
    let containers = list_instance(&vm.key, docker_api).await?;

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
  let kvm = host_config.kvm.unwrap_or(true);
  if kvm {
    args.push("-accel".into());
    args.push("kvm".into());
  }
  let cpu = host_config.cpu;
  let cpu = if cpu > 0 { cpu.to_string() } else { "2".into() };
  let cpu = cpu.clone();
  args.push("-smp".into());
  args.push(cpu.clone());
  let memory = host_config.memory;
  let memory = if memory > 0 {
    format!("{memory}M")
  } else {
    "2G".into()
  };
  args.push("-m".into());
  args.push(memory);

  let mut env: Vec<String> = Vec::new();
  env.push(format!("DELETE_SSH_KEY={disable_keygen}"));

  if let Some(user) = &vm.config.user {
    env.push(format!("USER={user}"));
  }
  if let Some(password) = &vm.config.password {
    env.push(format!("PASSWORD={password}"));
  }
  if let Some(ssh_key) = &vm.config.ssh_key {
    env.push(format!("SSH_KEY={ssh_key}"));
  }

  let config = bollard_next::container::Config {
    image: Some("nexthat/nanocl-qemu:0.1.0".into()),
    tty: Some(true),
    hostname: vm.config.hostname.clone(),
    env: Some(env),
    labels: Some(labels),
    cmd: Some(args),
    attach_stderr: Some(true),
    attach_stdin: Some(true),
    attach_stdout: Some(true),
    open_stdin: Some(true),
    stdin_once: Some(true),
    host_config: Some(HostConfig {
      network_mode: Some(vm.namespace_name.to_owned()),
      binds: Some(vec![format!("{vmimagespath}:/var/lib/nanocl/vms/images")]),
      devices: Some(vec![
        DeviceMapping {
          path_on_host: Some("/dev/kvm".into()),
          path_in_container: Some("/dev/kvm".into()),
          cgroup_permissions: Some("rwm".into()),
        },
        DeviceMapping {
          path_on_host: Some("/dev/net/tun".into()),
          path_in_container: Some("/dev/net/tun".into()),
          cgroup_permissions: Some("rwm".into()),
        },
      ]),
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

  Ok(())
}

pub async fn create(
  vm: &VmConfigPartial,
  namespace: &str,
  version: &str,
  state: &DaemonState,
) -> Result<Vm, HttpError> {
  let vm_key = utils::key::gen_key(namespace, &vm.name);

  let mut vm = vm.clone();
  if repositories::vm::find_by_key(&vm_key, &state.pool)
    .await
    .is_ok()
  {
    return Err(HttpError {
      status: StatusCode::CONFLICT,
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
      status: StatusCode::BAD_REQUEST,
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

pub async fn patch(
  cargo_key: &str,
  config: &VmConfigUpdate,
  version: &str,
  state: &DaemonState,
) -> Result<Vm, HttpError> {
  let vm = repositories::vm::find_by_key(cargo_key, &state.pool).await?;

  let old_config =
    repositories::vm_config::find_by_key(&vm.config_key, &state.pool).await?;

  let vm_partial = VmConfigPartial {
    name: config.name.to_owned().unwrap_or(vm.name.clone()),
    disk: old_config.disk,
    host_config: config
      .host_config
      .to_owned()
      .unwrap_or(old_config.host_config),
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
  };

  let container_name = format!("{}.v", &vm.key);

  stop(&vm, &state.docker_api).await?;

  state
    .docker_api
    .remove_container(&container_name, None::<RemoveContainerOptions>)
    .await?;

  let vm =
    repositories::vm::update_by_key(&vm.key, &vm_partial, version, &state.pool)
      .await?;

  let image =
    repositories::vm_image::find_by_name(&vm.config.disk.image, &state.pool)
      .await?;

  create_instance(&vm, &image, false, state).await?;
  // Update the vm
  start(&vm.key, &state.docker_api).await?;

  Ok(vm)
}
