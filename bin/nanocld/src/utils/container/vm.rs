use std::collections::HashMap;

use bollard_next::secret::{DeviceMapping, HostConfig};
use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  generic::ImagePullPolicy,
  process::{Process, ProcessKind},
  vm::Vm,
};

use crate::{
  models::{SystemState, VmImageDb},
  vars,
};

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
