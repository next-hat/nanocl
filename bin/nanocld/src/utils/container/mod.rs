use std::collections::HashMap;

use futures::StreamExt;
use futures_util::stream::FuturesUnordered;

use bollard_next::{
  container::{
    Config, RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
  },
  service::{DeviceMapping, HostConfig},
};
use nanocl_error::http::HttpResult;

use nanocl_stubs::{
  cargo::CargoKillOptions,
  generic::ImagePullPolicy,
  job::Job,
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
  vm::Vm,
};

use crate::{
  models::{
    CargoDb, JobDb, JobUpdateDb, ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb,
    SystemState, VmDb, VmImageDb,
  },
  repositories::generic::*,
  vars,
};

pub mod cargo;
mod image;
mod process;

/// Internal utils to emit an event when the state of a process kind changes
/// Eg: (job, cargo, vm)
async fn _emit(
  kind_key: &str,
  kind: &ProcessKind,
  action: NativeEventAction,
  state: &SystemState,
) -> HttpResult<()> {
  match kind {
    ProcessKind::Vm => {
      let vm = VmDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&vm, action).await;
    }
    ProcessKind::Cargo => {
      let cargo =
        CargoDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&cargo, action).await;
    }
    ProcessKind::Job => {
      JobDb::update_pk(
        kind_key,
        JobUpdateDb {
          updated_at: Some(chrono::Utc::now().naive_utc()),
        },
        &state.inner.pool,
      )
      .await?;
      let job =
        JobDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action_sync(&job, action).await;
    }
  }
  Ok(())
}

/// Delete a single instance (container) by his name
pub async fn delete_instance(
  pk: &str,
  opts: Option<RemoveContainerOptions>,
  state: &SystemState,
) -> HttpResult<()> {
  match state.inner.docker_api.remove_container(pk, opts).await {
    Ok(_) => {}
    Err(err) => match &err {
      bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: _,
      } => {
        log::error!("Error while deleting container {pk}: {err}");
        if *status_code != 404 {
          return Err(err.into());
        }
      }
      _ => {
        log::error!("Error while deleting container {pk}: {err}");
        return Err(err.into());
      }
    },
  };
  ProcessDb::del_by_pk(pk, &state.inner.pool).await?;
  Ok(())
}

/// Delete a group of instances (containers) by their names
pub async fn delete_instances(
  instances: &[String],
  state: &SystemState,
) -> HttpResult<()> {
  instances
    .iter()
    .map(|id| async {
      delete_instance(
        id,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
        state,
      )
      .await
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<HttpResult<()>>>()
    .await
    .into_iter()
    .collect::<HttpResult<()>>()
}

/// Kill instances (containers) by their kind key
/// Eg: kill a (job, cargo, vm)
pub async fn kill_by_kind_key(
  pk: &str,
  opts: &CargoKillOptions,
  state: &SystemState,
) -> HttpResult<()> {
  let processes = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .kill_container(&process.key, Some(opts.clone().into()))
      .await?;
  }
  Ok(())
}

/// Restart the group of process for a kind key
/// Eg: (job, cargo, vm, etc.)
/// When finished, a event is emitted to the system
pub async fn restart_instances(
  pk: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> HttpResult<()> {
  let processes = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .restart_container(&process.key, None)
      .await?;
  }
  _emit(pk, kind, NativeEventAction::Restart, state).await?;
  Ok(())
}

/// Stop the group of containers for a kind key
/// Eg: (job, cargo, vm)
/// When finished, a event is emitted to the system
pub async fn stop_instances(
  kind_pk: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> HttpResult<()> {
  let processes =
    ProcessDb::read_by_kind_key(kind_pk, &state.inner.pool).await?;
  log::debug!("stop_process_by_kind_pk: {kind_pk}");
  for process in processes {
    state
      .inner
      .docker_api
      .stop_container(
        &process.data.id.unwrap_or_default(),
        None::<StopContainerOptions>,
      )
      .await?;
  }
  ObjPsStatusDb::update_actual_status(
    kind_pk,
    &ObjPsStatusKind::Stop,
    &state.inner.pool,
  )
  .await?;
  _emit(kind_pk, kind, NativeEventAction::Stop, state).await?;
  Ok(())
}

/// Start the group of process for a kind key
/// Eg: (job, cargo, vm, etc.)
/// When finished, a event is emitted to the system
pub async fn start_instances(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> HttpResult<()> {
  let processes =
    ProcessDb::read_by_kind_key(kind_key, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .start_container(
        &process.data.id.unwrap_or_default(),
        None::<StartContainerOptions<String>>,
      )
      .await?;
  }
  ObjPsStatusDb::update_actual_status(
    kind_key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  _emit(kind_key, kind, NativeEventAction::Start, state).await?;
  Ok(())
}

/// Count the status for the given instances
/// Return a tuple with the total, failed, success and running instances
pub fn count_status(instances: &[Process]) -> (usize, usize, usize, usize) {
  let mut instance_failed = 0;
  let mut instance_success = 0;
  let mut instance_running = 0;
  for instance in instances {
    let container = &instance.data;
    let state = container.state.clone().unwrap_or_default();
    if state.restarting.unwrap_or_default() {
      instance_failed += 1;
      continue;
    }
    if state.running.unwrap_or_default() {
      instance_running += 1;
      continue;
    }
    if state.finished_at.unwrap() == "0001-01-01T00:00:00Z" {
      instance_running += 1;
      continue;
    }
    if let Some(exit_code) = state.exit_code {
      if exit_code == 0 {
        instance_success += 1;
      } else {
        instance_failed += 1;
      }
    }
    if let Some(error) = state.error {
      if !error.is_empty() {
        instance_failed += 1;
      }
    }
  }
  (
    instances.len(),
    instance_failed,
    instance_success,
    instance_running,
  )
}

/// Create a VM instance from a VM image
pub async fn create_vm_instance(
  vm: &Vm,
  image: &VmImageDb,
  disable_keygen: bool,
  state: &SystemState,
) -> HttpResult<Process> {
  let mut labels: HashMap<String, String> = HashMap::new();
  let img_path = format!("{}/vms/images", state.inner.config.state_dir);
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
    None => vars::VM_RUNTIME.to_owned(),
  };
  image::download(&image, None, ImagePullPolicy::IfNotPresent, vm, state)
    .await?;
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
          .unwrap_or("nanoclbr0".to_owned()),
      ),
      binds: Some(vec![format!("{img_path}:{img_path}")]),
      devices: Some(devices),
      cap_add: Some(vec!["NET_ADMIN".into()]),
      ..Default::default()
    }),
    ..Default::default()
  };
  let name = format!("{}.v", &vm.spec.vm_key);
  let process =
    process::create(&ProcessKind::Vm, &name, &vm.spec.vm_key, &spec, state)
      .await?;
  Ok(process)
}

/// Create process (container) for a job
async fn create_job_instance(
  name: &str,
  index: usize,
  container: &Config,
  state: &SystemState,
) -> HttpResult<Process> {
  let mut container = container.clone();
  let mut labels = container.labels.clone().unwrap_or_default();
  labels.insert("io.nanocl.j".to_owned(), name.to_owned());
  container.labels = Some(labels);
  let host_config = container.host_config.clone().unwrap_or_default();
  container.host_config = Some(HostConfig {
    network_mode: Some(
      host_config.network_mode.unwrap_or("nanoclbr0".to_owned()),
    ),
    ..host_config
  });
  let short_id = super::key::generate_short_id(6);
  let container_name = format!("{name}-{index}-{short_id}.j");
  process::create(&ProcessKind::Job, &container_name, name, &container, state)
    .await
}

/// Create processes (container) for a job
pub async fn create_job_instances(
  job: &Job,
  state: &SystemState,
) -> HttpResult<Vec<Process>> {
  let mut processes = Vec::new();
  for (index, container) in job.containers.iter().enumerate() {
    image::download(
      &container.image.clone().unwrap_or_default(),
      job.image_pull_secret.clone(),
      job.image_pull_policy.clone().unwrap_or_default(),
      job,
      state,
    )
    .await?;
    let process =
      create_job_instance(&job.name, index, container, state).await?;
    processes.push(process);
  }
  Ok(processes)
}

/// Emit a starting event to the system for the related process object (job, cargo, vm)
/// This will update the status of the process and emit a event
/// So the system start to start the group of processes in the background
pub async fn emit_starting(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> HttpResult<()> {
  log::debug!("starting {kind:?} {kind_key}");
  let current_status =
    ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  let wanted = if ProcessKind::Job == *kind {
    ObjPsStatusKind::Finish
  } else {
    ObjPsStatusKind::Start
  }
  .to_string();
  let status_update = ObjPsStatusUpdate {
    wanted: Some(wanted),
    prev_wanted: Some(current_status.wanted),
    actual: Some(ObjPsStatusKind::Starting.to_string()),
    prev_actual: Some(current_status.actual),
  };
  ObjPsStatusDb::update_pk(kind_key, status_update, &state.inner.pool).await?;
  _emit(kind_key, kind, NativeEventAction::Starting, state).await?;
  Ok(())
}

/// Emit a stopping event to the system for the related process object (job, cargo, vm)
/// This will update the status of the process and emit a event
/// So the system start to stop the group of processes in the background
pub async fn emit_stopping(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> HttpResult<()> {
  log::debug!("stopping {kind:?} {kind_key}");
  let current_status =
    ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  if current_status.actual == ObjPsStatusKind::Stop.to_string() {
    log::debug!("{kind:?} {kind_key} already stopped",);
    return Ok(());
  }
  let status_update = ObjPsStatusUpdate {
    wanted: Some(ObjPsStatusKind::Stop.to_string()),
    prev_wanted: Some(current_status.wanted),
    actual: Some(ObjPsStatusKind::Stopping.to_string()),
    prev_actual: Some(current_status.actual),
  };
  ObjPsStatusDb::update_pk(kind_key, status_update, &state.inner.pool).await?;
  _emit(kind_key, kind, NativeEventAction::Stopping, state).await?;
  Ok(())
}
