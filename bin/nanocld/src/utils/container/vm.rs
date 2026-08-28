use std::collections::HashMap;

use bollard_next::{
  container::{Config, StartContainerOptions, WaitContainerOptions},
  secret::{DeviceMapping, HostConfig},
};
use futures::StreamExt;

use nanocl_error::io::{FromIo, IoResult};
use nanocl_stubs::{
  generic::ImagePullPolicy,
  process::{Process, ProcessKind},
  system::NativeEventAction,
  vm::Vm,
};

use crate::{
  models::{ProcessDb, SystemState, VmDb},
  repositories::generic::*,
  utils, vars,
};

fn network_modes(vm: &Vm) -> Vec<String> {
  let mut modes = vec![super::network::resolve_network_mode(
    vm.spec.host_config.runtime_network.clone(),
  )];
  if let Some(init_container) = &vm.spec.init_container {
    modes.push(super::network::container_network_mode(init_container));
  }
  modes
}

async fn ensure_networks(vm: &Vm, state: &SystemState) -> IoResult<()> {
  super::network::ensure_networks(network_modes(vm), state).await
}

#[derive(Debug, PartialEq, Eq)]
enum VmProcessRole {
  Runtime,
  Init,
  Other,
}

fn vm_process_role(process_name: &str, runtime_name: &str) -> VmProcessRole {
  if process_name == runtime_name {
    VmProcessRole::Runtime
  } else if process_name.ends_with(".init.v") {
    VmProcessRole::Init
  } else {
    VmProcessRole::Other
  }
}

fn runtime_delete_targets(
  runtime_keys: Vec<String>,
  runtime_name: &str,
) -> Vec<String> {
  if runtime_keys.is_empty() {
    vec![runtime_name.to_owned()]
  } else {
    runtime_keys
  }
}

async fn create_init_container(
  vm: &Vm,
  init_container: &Config,
  state: &SystemState,
) -> IoResult<Process> {
  let mut init_container = init_container.clone();
  let host_config = init_container.host_config.take().unwrap_or_default();
  init_container.host_config = Some(HostConfig {
    network_mode: Some(super::network::resolve_network_mode(
      host_config.network_mode,
    )),
    ..host_config
  });
  let image = init_container.image.clone().unwrap_or_else(|| {
    vm.spec
      .host_config
      .runtime
      .clone()
      .unwrap_or_else(|| vars::VM_RUNTIME.to_owned())
  });
  init_container.image = Some(image.clone());
  super::image::download(
    &image,
    None,
    ImagePullPolicy::IfNotPresent,
    vm,
    state,
  )
  .await?;
  let mut labels = init_container.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl.v".to_owned(), vm.spec.vm_key.to_owned());
  labels.insert("io.nanocl.n".to_owned(), vm.namespace_name.to_owned());
  labels.insert("io.nanocl.init-v".to_owned(), "true".to_owned());
  labels.insert(
    "com.docker.compose.project".to_owned(),
    format!("nanocl_{}", vm.namespace_name),
  );
  init_container.labels = Some(labels);
  let short_id = utils::key::generate_short_id(6);
  let name =
    format!("{}.{}-{}.init.v", vm.namespace_name, vm.spec.name, short_id);
  let process = super::process::create(
    &ProcessKind::Vm,
    &name,
    &vm.spec.vm_key,
    &init_container,
    state,
  )
  .await?;
  Ok(process)
}

async fn start_init_container(
  process: &Process,
  state: &SystemState,
) -> IoResult<()> {
  state
    .inner
    .docker_api
    .start_container(&process.name, None::<StartContainerOptions<String>>)
    .await
    .map_err(|err| err.map_err_context(|| "InitContainer"))?;
  let options = Some(WaitContainerOptions {
    condition: "not-running",
  });
  let mut stream = state
    .inner
    .docker_api
    .wait_container(&process.name, options);
  while let Some(wait_status) = stream.next().await {
    match wait_status {
      Ok(wait_status) => {
        if wait_status.status_code != 0 {
          let error = match wait_status.error {
            Some(error) => error.message.unwrap_or("Unknown error".to_owned()),
            None => "Unknown error".to_owned(),
          };
          return Err(nanocl_error::io::IoError::interrupted(
            "InitContainer",
            &format!("{error} {}", wait_status.status_code),
          ));
        }
      }
      Err(err) => {
        return Err(nanocl_error::io::IoError::interrupted(
          "InitContainer",
          &format!("{err}"),
        ));
      }
    }
  }
  Ok(())
}

/// Create a VM instance
///
pub async fn create_instance(
  vm: &Vm,
  disable_keygen: bool,
  state: &SystemState,
) -> IoResult<Process> {
  // Ensure runtime and optional init networks before either container exists.
  ensure_networks(vm, state).await?;
  let mut labels: HashMap<String, String> = HashMap::new();
  labels.insert("io.nanocl.v".to_owned(), vm.spec.vm_key.clone());
  labels.insert("io.nanocl.n".to_owned(), vm.namespace_name.clone());
  labels.insert("io.nanocl.not-init-c".to_owned(), "true".to_owned());
  let mut args: Vec<String> =
    vec!["-hda".into(), vm.spec.image.clone(), "--nographic".into()];
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
  if let Some(init_container) = &vm.spec.init_container {
    let process = create_init_container(vm, init_container, state).await?;
    start_init_container(&process, state).await?;
  }
  super::image::download(
    &image,
    None,
    ImagePullPolicy::IfNotPresent,
    vm,
    state,
  )
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
      network_mode: Some(super::network::resolve_network_mode(
        vm.spec.host_config.runtime_network.clone(),
      )),
      binds: Some(vec![format!("{}:{}", vm.spec.image, vm.spec.image)]),
      devices: Some(devices),
      cap_add: Some(vec!["NET_ADMIN".into()]),
      ..Default::default()
    }),
    ..Default::default()
  };
  let name = format!("{}.v", &vm.spec.vm_key);
  let process = super::process::create(
    &ProcessKind::Vm,
    &name,
    &vm.spec.vm_key,
    &spec,
    state,
  )
  .await?;
  Ok(process)
}

/// Start VM instance
///
pub async fn start(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  ensure_networks(&vm, state).await?;
  let processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  if processes.is_empty() {
    create_instance(&vm, true, state).await?;
  }
  super::process::start_instances(
    &vm.spec.vm_key,
    &ProcessKind::Vm,
    true,
    state,
  )
  .await?;
  Ok(())
}

/// Delete VM instance and the VM itself from the database
///
pub async fn delete(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes =
    ProcessDb::read_by_kind_key(key, None, &state.inner.pool).await?;
  super::process::delete_instances(
    &processes
      .into_iter()
      .map(|p| p.key)
      .collect::<Vec<String>>(),
    state,
  )
  .await?;
  VmDb::clear_by_pk(&vm.spec.vm_key, &state.inner.pool).await?;
  state
    .emit_normal_native_action_sync(&vm, NativeEventAction::Destroy)
    .await;
  Ok(())
}

/// Update the VM
///
pub async fn update(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  // Fail before deleting the existing VM runtime.
  ensure_networks(&vm, state).await?;
  let runtime_name = format!("{}.v", &vm.spec.vm_key);
  let old_processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  let old_runtime_keys = old_processes
    .iter()
    .filter(|process| {
      vm_process_role(&process.name, &runtime_name) == VmProcessRole::Runtime
    })
    .map(|process| process.key.clone())
    .collect::<Vec<_>>();
  let old_init_keys = old_processes
    .iter()
    .filter(|process| {
      vm_process_role(&process.name, &runtime_name) == VmProcessRole::Init
    })
    .map(|process| process.key.clone())
    .collect::<Vec<_>>();
  let runtime_targets = runtime_delete_targets(old_runtime_keys, &runtime_name);
  super::process::delete_instances(&runtime_targets, state).await?;
  create_instance(&vm, false, state).await?;
  super::process::start_instances(key, &ProcessKind::Vm, true, state).await?;
  if !old_init_keys.is_empty()
    && let Err(err) =
      super::process::delete_instances(&old_init_keys, state).await
  {
    // The replacement is already running. Do not turn successful replacement
    // into a failed update solely because cleanup of exited init containers
    // was incomplete.
    log::warn!("Unable to clean previous VM init containers: {err}");
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::vm_spec::VmHostConfig;

  fn container_on(network: &str) -> Config {
    Config {
      host_config: Some(HostConfig {
        network_mode: Some(network.to_owned()),
        ..Default::default()
      }),
      ..Default::default()
    }
  }

  #[test]
  fn network_preflight_includes_vm_runtime_and_init_container_modes() {
    let mut vm = Vm::default();
    vm.spec.host_config.runtime_network = Some("private-vm".to_owned());
    vm.spec.init_container = Some(container_on("private-init"));

    assert_eq!(
      network_modes(&vm),
      vec!["private-vm".to_owned(), "private-init".to_owned()]
    );
  }

  #[test]
  fn vm_network_mode_accepts_canonical_and_legacy_state_fields() {
    for field in ["NetworkMode", "RuntimeNetwork"] {
      let config = serde_yaml::from_str::<VmHostConfig>(&format!(
        "Cpu: 1\nMemory: 512\n{field}: private-vm\n"
      ))
      .unwrap();
      assert_eq!(config.runtime_network.as_deref(), Some("private-vm"));
    }

    let value = serde_json::to_value(VmHostConfig {
      runtime_network: Some("private-vm".to_owned()),
      ..Default::default()
    })
    .unwrap();
    assert_eq!(value["NetworkMode"], "private-vm");
    assert!(value.get("RuntimeNetwork").is_none());
  }

  #[test]
  fn vm_update_process_roles_keep_runtime_and_init_ids_separate() {
    let runtime_name = "global.ubuntu.v";
    assert_eq!(
      vm_process_role(runtime_name, runtime_name),
      VmProcessRole::Runtime
    );
    assert_eq!(
      vm_process_role("global.ubuntu-a1b2c3.init.v", runtime_name),
      VmProcessRole::Init
    );
    let prefixed_runtime_name = "init-services.ubuntu.v";
    assert_eq!(
      vm_process_role(prefixed_runtime_name, prefixed_runtime_name),
      VmProcessRole::Runtime
    );
    assert_eq!(
      vm_process_role("tmp-global.ubuntu.v", runtime_name),
      VmProcessRole::Other
    );
  }

  #[test]
  fn vm_update_prefers_ids_but_keeps_an_orphan_name_fallback() {
    assert_eq!(
      runtime_delete_targets(vec!["runtime-id".to_owned()], "global.ubuntu.v"),
      vec!["runtime-id".to_owned()]
    );
    assert_eq!(
      runtime_delete_targets(Vec::new(), "global.ubuntu.v"),
      vec!["global.ubuntu.v".to_owned()]
    );
  }
}
