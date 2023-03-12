use std::collections::HashMap;

use ntex::http::StatusCode;
use bollard_next::Docker;
use bollard_next::service::{HostConfig, DeviceMapping, ContainerSummary};
use bollard_next::container::{
  CreateContainerOptions, StartContainerOptions, ListContainersOptions,
  StopContainerOptions,
};

use nanocl_stubs::vm::{Vm, VmSummary, VmInspect};
use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::vm_config::VmConfigPartial;

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::{Pool, VmDbModel};

pub async fn start(
  vm_key: &str,
  docker_api: &Docker,
) -> Result<(), HttpResponseError> {
  let container_name = format!("{}.vm", vm_key);
  docker_api
    .start_container(&container_name, None::<StartContainerOptions<String>>)
    .await
    .map_err(|e| HttpResponseError {
      msg: format!("Unable to start container got error : {e}"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;

  Ok(())
}

/// Stop a VM by his model
pub async fn stop(
  vm: &VmDbModel,
  docker_api: &Docker,
) -> Result<(), HttpResponseError> {
  let container_name = format!("{}.vm", vm.key);
  docker_api
    .stop_container(&container_name, None::<StopContainerOptions>)
    .await
    .map_err(|e| HttpResponseError {
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
) -> Result<(), HttpResponseError> {
  let vm = repositories::vm::find_by_key(vm_key, pool).await?;

  stop(&vm, docker_api).await
}

pub async fn inspect(
  vm_key: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<VmInspect, HttpResponseError> {
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
) -> Result<Vec<ContainerSummary>, HttpResponseError> {
  let label = format!("io.nanocl.vm={vm_key}");
  let mut filters: HashMap<&str, Vec<&str>> = HashMap::new();
  filters.insert("label", vec![&label]);
  let options = Some(ListContainersOptions {
    all: true,
    filters,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await.map_err(|e| {
    HttpResponseError {
      msg: format!("Unable to list containers got error : {e}"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    }
  })?;

  Ok(containers)
}

pub async fn delete(
  vm_key: &str,
  force: bool,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<(), HttpResponseError> {
  let vm = repositories::vm::inspect_by_key(vm_key, pool).await?;

  let options = bollard_next::container::RemoveContainerOptions {
    force,
    ..Default::default()
  };

  let container_name = format!("{}.vm", vm_key);
  docker_api
    .remove_container(&container_name, Some(options))
    .await
    .map_err(|e| HttpResponseError {
      msg: format!("Unable to remove container got error : {e}"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;

  utils::vm_image::delete(&vm.config.image, pool).await?;
  repositories::vm_config::delete_by_vm_key(vm.key, pool).await?;
  repositories::vm::delete_by_key(vm_key, pool).await?;

  Ok(())
}

pub async fn list(
  nsp: &str,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<Vec<VmSummary>, HttpResponseError> {
  let namespace =
    repositories::namespace::find_by_name(nsp.to_owned(), pool).await?;

  let vmes = repositories::vm::find_by_namespace(&namespace, pool).await?;

  let mut vm_summaries = Vec::new();

  for vm in vmes {
    let config =
      repositories::vm_config::find_by_key(vm.config_key, pool).await?;
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

pub async fn create(
  mut vm: VmConfigPartial,
  namespace: &str,
  version: String,
  daemon_conf: &DaemonConfig,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<Vm, HttpResponseError> {
  let vm_key = utils::key::gen_key(namespace, &vm.name);

  if repositories::vm::find_by_key(&vm_key, pool).await.is_ok() {
    return Err(HttpResponseError {
      status: StatusCode::CONFLICT,
      msg: format!(
        "VM with name {} already exists in namespace {namespace}",
        vm.name
      ),
    });
  }
  let vmimagespath = format!("{}/vms/images", daemon_conf.state_dir);
  let image = repositories::vm_image::find_by_name(&vm.image, pool).await?;
  let snapname = format!("{}.{vm_key}", &image.name);
  let image =
    utils::vm_image::create_snap(&snapname, &image, daemon_conf, pool).await?;

  // Use the snapshot image
  vm.image = image.name;

  let vm = repositories::vm::create(namespace, vm, &version, pool).await?;

  let mut labels = HashMap::new();
  labels.insert("io.nanocl.vm", vm.key.as_str());
  labels.insert("io.nanocl.vmnsp", namespace);

  let config = bollard_next::container::Config {
    image: Some("nanocl-qemu:dev"),
    tty: Some(true),
    labels: Some(labels),
    cmd: Some(vec![
      "-accel",
      "kvm",
      "-m",
      "4G",
      "-smp 4",
      "-hda",
      &image.path,
      "--nographic",
    ]),
    attach_stderr: Some(true),
    attach_stdin: Some(true),
    attach_stdout: Some(true),
    open_stdin: Some(true),
    stdin_once: Some(true),
    host_config: Some(HostConfig {
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
    name: format!("{}.vm", &vm.key),
    ..Default::default()
  });

  docker_api.create_container(options, config).await?;

  Ok(vm)
}
