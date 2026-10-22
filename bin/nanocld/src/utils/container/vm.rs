use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use bollard_next::{
  container::{Config, StartContainerOptions, WaitContainerOptions},
  network::InspectNetworkOptions,
  secret::{DeviceMapping, HostConfig},
};
use futures::StreamExt;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  generic::ImagePullPolicy,
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
  vm::Vm,
  vm_spec::VmCloudInitUserData,
};

use crate::{
  models::{ObjPsStatusDb, ProcessDb, SystemState, VmDb},
  repositories::generic::*,
  utils, vars,
};

mod lifecycle;
mod resize;
pub(crate) use lifecycle::{
  control_process, lifecycle_lock, restart_with_options, runtime_status,
  socket_directory, stop_with_options,
};
pub(crate) use resize::{resize, runtime_resources, validate_boot_spec};

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
  let mode = super::network::resolve_network_mode(
    vm.spec.host_config.runtime_network.clone(),
  );
  validate_network_mode(&mode)?;
  super::network::ensure_networks(network_modes(vm), state).await?;
  let network = state
    .inner
    .docker_api
    .inspect_network(&mode, None::<InspectNetworkOptions<String>>)
    .await
    .map_err(|err| err.map_err_context(|| "VM network"))?;
  if network.driver.as_deref() != Some("bridge")
    || network.internal == Some(true)
    || network.enable_ipv6 == Some(true)
  {
    return Err(IoError::invalid_input(
      "VM network",
      "VM networking requires an external IPv4-only Docker bridge network",
    ));
  }
  validate_link_net_iface(
    vm.spec.host_config.link_net_iface.as_deref(),
    network.options.as_ref(),
  )
}

fn validate_link_net_iface(
  interface: Option<&str>,
  options: Option<&HashMap<String, String>>,
) -> IoResult<()> {
  let Some(interface) = interface else {
    return Ok(());
  };
  let prefix = options
    .and_then(|options| {
      options.get("com.docker.network.container_iface_prefix")
    })
    .map(String::as_str)
    .filter(|prefix| !prefix.is_empty())
    .unwrap_or("eth");
  // Each VM starts with one endpoint in its own Docker network namespace.
  let expected = format!("{prefix}0");
  if interface != expected {
    return Err(IoError::invalid_input(
      "VM network",
      &format!(
        "LinkNetIface must match the Docker uplink {expected}; omit it for automatic discovery"
      ),
    ));
  }
  Ok(())
}

fn validate_network_mode(mode: &str) -> IoResult<()> {
  if mode.is_empty()
    || matches!(mode, "host" | "none" | "default")
    || mode.starts_with("container:")
  {
    return Err(IoError::invalid_input(
      "VM network",
      "VMs require a dedicated container network namespace on a Docker bridge",
    ));
  }
  Ok(())
}

/// Validate VM-specific constraints before accepting a desired spec or touching
/// its currently running container. Init containers retain their own modes.
pub(crate) fn validate_spec(
  spec: &nanocl_stubs::vm_spec::VmSpecPartial,
) -> IoResult<()> {
  validate_host_config(&spec.host_config.clone().unwrap_or_default())?;
  cloud_init_user_data(spec.cloud_init_user_data.as_ref())?;
  let generated =
    spec.cloud_init.unwrap_or(true) && spec.cloud_init_user_data.is_none();
  validate_cloud_init_user(generated, spec.user.as_deref())?;
  validate_cloud_init_credentials(
    generated,
    spec.password.as_deref(),
    spec.ssh_key.as_deref(),
  )
}

fn cloud_init_user_data(
  data: Option<&VmCloudInitUserData>,
) -> IoResult<Option<String>> {
  let Some(data) = data else {
    return Ok(None);
  };
  let data = match data {
    VmCloudInitUserData::Raw(data) => data.clone(),
    VmCloudInitUserData::CloudConfig(config) => {
      // JSON is a YAML subset. This preserves typed values without treating
      // user strings as YAML syntax or changing raw user-data formatting.
      format!("#cloud-config\n{}\n", serde_json::to_string(config)?)
    }
  };
  if data.len() > 65_536 || data.contains('\0') {
    return Err(IoError::invalid_input(
      "VM provisioning",
      "CloudInitUserData must render to at most 65536 UTF-8 bytes without NUL characters",
    ));
  }
  Ok(Some(data))
}

fn validate_cloud_init_user(enabled: bool, user: Option<&str>) -> IoResult<()> {
  if !enabled {
    return Ok(());
  }
  // Match the runtime's ${USER:-cloud} fallback and account-name contract.
  let user = user.filter(|user| !user.is_empty()).unwrap_or("cloud");
  if !user.bytes().enumerate().all(|(index, character)| {
    character.is_ascii_lowercase()
      || character == b'_'
      || (index > 0 && (character.is_ascii_digit() || character == b'-'))
  }) {
    return Err(IoError::invalid_input(
      "VM provisioning",
      "User must start with a lowercase ASCII letter or underscore and contain only lowercase ASCII letters, digits, underscores or hyphens when CloudInit is enabled",
    ));
  }
  Ok(())
}

fn validate_cloud_init_credentials(
  enabled: bool,
  password: Option<&str>,
  ssh_key: Option<&str>,
) -> IoResult<()> {
  if !enabled {
    return Ok(());
  }
  for (field, value) in [("Password", password), ("SshKey", ssh_key)] {
    if value.is_some_and(|value| value.contains(['\r', '\n'])) {
      return Err(IoError::invalid_input(
        "VM provisioning",
        &format!(
          "{field} must not contain carriage returns or newlines when generated cloud-init is enabled"
        ),
      ));
    }
  }
  Ok(())
}

fn validate_host_config(
  config: &nanocl_stubs::vm_spec::VmHostConfig,
) -> IoResult<()> {
  resize::configured_resources(config)?;
  validate_network_mode(&super::network::resolve_network_mode(
    config.runtime_network.clone(),
  ))?;
  let interface = config.link_net_iface.as_deref().unwrap_or("eth0");
  if interface.is_empty()
    || interface.len() > 15
    || !interface
      .bytes()
      .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
  {
    return Err(IoError::invalid_input(
      "VM network",
      "LinkNetIface must be an interface name containing letters, digits, underscores or hyphens",
    ));
  }
  if let Some(servers) = &config.dns {
    if servers.is_empty()
      || servers.iter().any(|server| {
        server
          .parse::<std::net::Ipv4Addr>()
          .map(|ip| {
            ip.is_unspecified()
              || ip.is_multicast()
              || ip == std::net::Ipv4Addr::new(169, 254, 254, 1)
          })
          .unwrap_or(true)
      })
    {
      return Err(IoError::invalid_input(
        "VM DNS",
        "Dns must contain IPv4 resolvers other than the private DHCP helper 169.254.254.1",
      ));
    }
  }
  Ok(())
}

fn runtime_sysctls() -> HashMap<String, String> {
  let mut sysctls = HashMap::from([("net.ipv4.ip_forward".into(), "1".into())]);
  for scope in ["all", "default"] {
    for (setting, value) in [
      ("proxy_arp", "1"),
      ("accept_local", "1"),
      ("arp_ignore", "1"),
      ("send_redirects", "0"),
      ("rp_filter", "0"),
    ] {
      sysctls.insert(format!("net.ipv4.conf.{scope}.{setting}"), value.into());
    }
  }
  sysctls
}

async fn ensure_runtime_image(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<String> {
  let image = vm
    .spec
    .host_config
    .runtime
    .clone()
    .unwrap_or_else(|| vars::VM_RUNTIME.to_owned());
  super::image::download(
    &image,
    None,
    ImagePullPolicy::IfNotPresent,
    vm,
    state,
  )
  .await?;
  let inspect = state
    .inner
    .docker_api
    .inspect_image(&image)
    .await
    .map_err(|err| err.map_err_context(|| "VM runtime"))?;
  let labels = inspect
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  validate_runtime_labels(vm, labels)?;
  Ok(image)
}

fn validate_runtime_labels(
  vm: &Vm,
  labels: Option<&HashMap<String, String>>,
) -> IoResult<()> {
  if labels
    .and_then(|labels| labels.get("io.nanocl.vm-resize"))
    .map(String::as_str)
    != Some("1")
  {
    return Err(IoError::invalid_input(
      "VM runtime",
      "The image must implement io.nanocl.vm-resize=1; select the matching nanocl-qemu runtime",
    ));
  }
  let supported = labels
    .and_then(|labels| labels.get("io.nanocl.vm-runtime-api"))
    .map(String::as_str)
    == Some("0.19");
  if !supported {
    return Err(IoError::invalid_input(
      "VM runtime",
      "The image must implement io.nanocl.vm-runtime-api=0.19; build or select the matching nanocl-qemu runtime",
    ));
  }
  if labels
    .and_then(|labels| labels.get("io.nanocl.vm-qmp"))
    .map(String::as_str)
    != Some("1")
  {
    return Err(IoError::invalid_input(
      "VM runtime",
      "The image must implement io.nanocl.vm-qmp=1; pull and select the matching nanocl-qemu runtime",
    ));
  }
  if vm.spec.cloud_init.unwrap_or(true)
    && vm.spec.cloud_init_user_data.is_some()
    && labels
      .and_then(|labels| labels.get("io.nanocl.vm-cloud-init-user-data"))
      .map(String::as_str)
      != Some("1")
  {
    return Err(IoError::invalid_input(
      "VM runtime",
      "CloudInitUserData requires an image with io.nanocl.vm-cloud-init-user-data=1; select the matching nanocl-qemu runtime",
    ));
  }
  Ok(())
}

fn runtime_env(vm: &Vm, disable_keygen: bool) -> IoResult<Vec<String>> {
  validate_host_config(&vm.spec.host_config)?;
  let user_data = cloud_init_user_data(vm.spec.cloud_init_user_data.as_ref())?;
  let generated = vm.spec.cloud_init.unwrap_or(true) && user_data.is_none();
  validate_cloud_init_user(generated, vm.spec.user.as_deref())?;
  validate_cloud_init_credentials(
    generated,
    vm.spec.password.as_deref(),
    vm.spec.ssh_key.as_deref(),
  )?;
  let mac = vm.spec.mac_address.as_deref().ok_or_else(|| {
    IoError::invalid_input("VM MAC", "A persisted MAC address is required")
  })?;
  let mut envs = vec![
    "NANOCL_MANAGED_RESOURCES=true".to_owned(),
    format!("MAC={mac}"),
    "NANOCL_NETWORK_MODE=bridge".to_owned(),
    format!("CLOUD_INIT={}", vm.spec.cloud_init.unwrap_or(true)),
    format!("CONSOLE={}", vm.spec.console.unwrap_or(false)),
    format!("DELETE_SSH_KEY={disable_keygen}"),
    format!("INSTANCE_ID={}", vm.spec.vm_key),
  ];
  if let Some(interface) = &vm.spec.host_config.link_net_iface {
    envs.push(format!("DEFAULT_INTERFACE={interface}"));
  }
  if let Some(servers) = &vm.spec.host_config.dns {
    envs.push(format!("DNS={}", servers.join(",")));
  }
  if vm.spec.cloud_init.unwrap_or(true) {
    if let Some(data) = user_data {
      envs.push(format!("CLOUD_INIT_USER_DATA={data}"));
    } else {
      for (key, value) in [
        ("USER", &vm.spec.user),
        ("PASSWORD", &vm.spec.password),
        ("SSH_KEY", &vm.spec.ssh_key),
      ] {
        if let Some(value) = value {
          envs.push(format!("{key}={value}"));
        }
      }
    }
  }
  Ok(envs)
}

fn console_directory(vm: &Vm, state: &SystemState) -> IoResult<PathBuf> {
  // Resource keys come from the API; check again before using one as a path.
  let key = &vm.spec.vm_key;
  if key.is_empty()
    || key == "."
    || key == ".."
    || key.contains(['/', '\\', '\0'])
  {
    return Err(IoError::invalid_input("VM runtime", "Invalid VM key"));
  }
  let path = PathBuf::from(&state.inner.config.state_dir)
    .join("run/vms")
    .join(key)
    .join(vm.spec.key.to_string());
  if path.join("console.sock").as_os_str().len() >= 108 {
    return Err(IoError::invalid_input(
      "VM runtime",
      "VM socket path exceeds the Unix socket limit; use a shorter state directory or VM name",
    ));
  }
  Ok(path)
}

async fn prepare_console_directory(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<PathBuf> {
  let path = console_directory(vm, state)?;
  tokio::fs::create_dir_all(&state.inner.config.state_dir).await?;
  let mut parent =
    tokio::fs::canonicalize(&state.inner.config.state_dir).await?;
  for component in ["run", "vms", &vm.spec.vm_key, &vm.spec.key.to_string()] {
    let directory = parent.join(component);
    match tokio::fs::create_dir(&directory).await {
      Ok(()) => {}
      Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
      Err(err) => return Err(err.into()),
    }
    if tokio::fs::canonicalize(&directory).await? != directory
      || !tokio::fs::metadata(&directory).await?.is_dir()
    {
      return Err(IoError::invalid_input(
        "VM console",
        "Console directory must not redirect through symlinks",
      ));
    }
    parent = directory;
  }
  tokio::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
    .await?;
  // The daemon's state directory is mounted at the same path on the Docker host.
  if parent != path {
    return Err(IoError::invalid_input(
      "VM console",
      "Console state directory must be an absolute canonical path",
    ));
  }
  Ok(path)
}

async fn remove_console_directory(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<()> {
  let path = console_directory(vm, state)?;
  match tokio::fs::canonicalize(&path).await {
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(err) => return Err(err.into()),
    Ok(canonical) if canonical != path => {
      return Err(IoError::invalid_input(
        "VM console",
        "Refusing redirected console cleanup",
      ));
    }
    Ok(_) => {}
  }
  for socket in ["console.sock", "qmp.sock", "resources.json"] {
    match tokio::fs::remove_file(path.join(socket)).await {
      Ok(()) => {}
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
      Err(err) => return Err(err.into()),
    }
  }
  // Atomic resource writes can leave an owned temporary file after a daemon
  // crash. Remove only that exact generated filename shape, never recurse.
  let mut entries = tokio::fs::read_dir(&path).await?;
  while let Some(entry) = entries.next_entry().await? {
    let name = entry.file_name();
    if name
      .to_str()
      .and_then(|name| name.strip_prefix("resources-"))
      .and_then(|name| name.strip_suffix(".tmp"))
      .and_then(|name| uuid::Uuid::parse_str(name).ok())
      .is_some()
    {
      tokio::fs::remove_file(entry.path()).await?;
    }
  }
  tokio::fs::remove_dir(&path).await?;
  // Other generations can still exist. Only remove an empty VM directory.
  if let Some(parent) = path.parent() {
    let _ = tokio::fs::remove_dir(parent).await;
  }
  Ok(())
}

async fn cleanup_process_consoles(
  vm: &Vm,
  processes: &[Process],
  state: &SystemState,
) -> IoResult<()> {
  for process in processes {
    let Some(labels) = process
      .data
      .config
      .as_ref()
      .and_then(|config| config.labels.as_ref())
    else {
      continue;
    };
    if process.kind != ProcessKind::Vm
      || process.kind_key != vm.spec.vm_key
      || labels.get("io.nanocl.v") != Some(&vm.spec.vm_key)
    {
      continue;
    }
    let Some(generation) = labels
      .get("io.nanocl.vm-spec")
      .and_then(|key| uuid::Uuid::parse_str(key).ok())
    else {
      continue;
    };
    let mut old_vm = vm.clone();
    old_vm.spec.key = generation;
    remove_console_directory(&old_vm, state).await?;
  }
  Ok(())
}

/// Include generations left by failed creates or externally removed containers,
/// which no longer have a process row. Never recurse into arbitrary entries.
async fn cleanup_all_consoles(vm: &Vm, state: &SystemState) -> IoResult<()> {
  // Derive the owned VM parent without imposing the leaf socket length limit
  // on VMs which have never enabled a console.
  let key = &vm.spec.vm_key;
  if key.is_empty()
    || matches!(key.as_str(), "." | "..")
    || key.contains(['/', '\\', '\0'])
  {
    return Err(IoError::invalid_input("VM console", "Invalid VM key"));
  }
  let parent = PathBuf::from(&state.inner.config.state_dir)
    .join("run/vms")
    .join(key);
  match tokio::fs::canonicalize(&parent).await {
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(err) => return Err(err.into()),
    Ok(canonical) if canonical != parent => {
      return Err(IoError::invalid_input(
        "VM console",
        "Refusing redirected console cleanup",
      ));
    }
    Ok(_) => {}
  }
  let mut entries = tokio::fs::read_dir(&parent).await?;
  while let Some(entry) = entries.next_entry().await? {
    let name = entry.file_name();
    let Some(generation) = name
      .to_str()
      .and_then(|value| uuid::Uuid::parse_str(value).ok())
    else {
      continue;
    };
    if name != generation.to_string().as_str() {
      continue;
    }
    let mut old_vm = vm.clone();
    old_vm.spec.key = generation;
    remove_console_directory(&old_vm, state).await?;
  }
  match tokio::fs::remove_dir(&parent).await {
    Ok(()) => {}
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
    Err(err) => return Err(err.into()),
  }
  Ok(())
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

fn owned_runtime_id(
  vm: &Vm,
  inspect: &bollard_next::models::ContainerInspectResponse,
) -> IoResult<String> {
  let labels = inspect
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  if inspect.name.as_deref() != Some(&format!("/{}.v", vm.spec.vm_key))
    || labels.and_then(|labels| labels.get("io.nanocl.v"))
      != Some(&vm.spec.vm_key)
    || labels.and_then(|labels| labels.get("io.nanocl.n"))
      != Some(&vm.namespace_name)
    || labels
      .and_then(|labels| labels.get("io.nanocl.init-v"))
      .map(String::as_str)
      == Some("true")
  {
    return Err(IoError::invalid_input(
      "VM runtime",
      "Refusing to remove a runtime with a different owner",
    ));
  }
  inspect
    .id
    .clone()
    .filter(|id| !id.is_empty())
    .ok_or_else(|| {
      IoError::invalid_input("VM runtime", "The runtime has no Docker identity")
    })
}

async fn inspect_runtime_id(
  vm: &Vm,
  state: &SystemState,
) -> IoResult<Option<String>> {
  match state
    .inner
    .docker_api
    .inspect_container(&format!("{}.v", vm.spec.vm_key), None)
    .await
  {
    Ok(inspect) => owned_runtime_id(vm, &inspect).map(Some),
    Err(bollard_next::errors::Error::DockerResponseServerError {
      status_code: 404,
      ..
    }) => Ok(None),
    Err(err) => Err(*err.map_err_context(|| "VM runtime")),
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
  let vm = VmDb::ensure_mac_address(&vm.spec.vm_key, state)
    .await
    .map_err(|err| IoError::invalid_input("VM MAC", &err.to_string()))?;
  let vm = &vm;
  let envs = runtime_env(vm, disable_keygen)?;
  // Ensure runtime and optional init networks before either container exists.
  ensure_networks(vm, state).await?;
  let image = ensure_runtime_image(vm, state).await?;
  let mut labels: HashMap<String, String> = HashMap::new();
  labels.insert("io.nanocl.v".to_owned(), vm.spec.vm_key.clone());
  labels.insert("io.nanocl.n".to_owned(), vm.namespace_name.clone());
  labels.insert("io.nanocl.not-init-c".to_owned(), "true".to_owned());
  labels.insert("io.nanocl.vm-spec".to_owned(), vm.spec.key.to_string());
  labels.insert("io.nanocl.vm-qmp".to_owned(), "1".to_owned());
  labels.insert("io.nanocl.vm-resize".to_owned(), "1".to_owned());
  labels.insert(
    "io.nanocl.vm-console".to_owned(),
    vm.spec.console.unwrap_or(false).to_string(),
  );
  let mut args: Vec<String> = vec!["-hda".into(), vm.spec.image.clone()];
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
  if let Some(init_container) = &vm.spec.init_container {
    let process = create_init_container(vm, init_container, state).await?;
    if let Err(err) = start_init_container(&process, state).await {
      if let Err(cleanup) =
        super::process::delete_instances(&[process.key], state).await
      {
        log::warn!("Unable to remove failed VM init container: {cleanup}");
      }
      return Err(err);
    }
  }
  let mut binds = vec![format!("{}:{}", vm.spec.image, vm.spec.image)];
  // A lost process row must not let a failed create rewrite a live runtime's
  // restart projection before Docker rejects the duplicate container name.
  if inspect_runtime_id(vm, state).await?.is_some() {
    return Err(IoError::with_context(
      "VM create",
      std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "VM runtime already exists",
      ),
    ));
  }
  let directory = prepare_console_directory(vm, state).await?;
  resize::write_resources(
    &directory,
    &resize::configured_resources(&host_config)?,
  )
  .await?;
  binds.push(format!("{}:/run/nanocl-vm", directory.display()));
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
      binds: Some(binds),
      devices: Some(devices),
      cap_add: Some(vec!["NET_ADMIN".into()]),
      sysctls: Some(runtime_sysctls()),
      ..Default::default()
    }),
    ..Default::default()
  };
  let name = format!("{}.v", &vm.spec.vm_key);
  let result = super::process::create(
    &ProcessKind::Vm,
    &name,
    &vm.spec.vm_key,
    &spec,
    state,
  )
  .await;
  if result.is_err() {
    // A name-conflict can mean a live runtime exists without its process row.
    // Never unlink its socket (or its mounted directory) on a create error.
    if matches!(
      state.inner.docker_api.inspect_container(&name, None).await,
      Err(bollard_next::errors::Error::DockerResponseServerError {
        status_code: 404,
        ..
      })
    ) {
      let _ = remove_console_directory(vm, state).await;
    }
  }
  result
}

/// Start VM instance
///
pub async fn start(key: &str, state: &SystemState) -> IoResult<()> {
  let _guard = lifecycle_lock(key).lock_owned().await;
  let result = start_unlocked(key, state).await;
  record_task_failure(key, result, state).await
}

// Call while holding the operation's lifecycle guard. The asynchronous task
// error callback runs after release and must not overwrite a newer operation.
async fn record_task_failure(
  key: &str,
  result: IoResult<()>,
  state: &SystemState,
) -> IoResult<()> {
  if result.is_err() {
    ObjPsStatusDb::update_actual_status(
      key,
      &ObjPsStatusKind::Fail,
      &state.inner.pool,
    )
    .await?;
  }
  result
}

async fn start_unlocked(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  ensure_networks(&vm, state).await?;
  let processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  let runtime_name = format!("{}.v", vm.spec.vm_key);
  if !processes.iter().any(|process| process.name == runtime_name) {
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
  let _guard = lifecycle_lock(key).lock_owned().await;
  let result = delete_unlocked(key, state).await;
  record_task_failure(key, result, state).await
}

async fn delete_unlocked(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes =
    ProcessDb::read_by_kind_key(key, None, &state.inner.pool).await?;
  let mut targets = processes.iter().map(|p| p.key.clone()).collect::<Vec<_>>();
  // A Docker runtime can survive a failed process-row write or reconciliation.
  // Verify its owner and use its immutable ID before releasing the VM identity.
  if let Some(id) = inspect_runtime_id(&vm, state).await? {
    if !targets.contains(&id) {
      targets.push(id);
    }
  }
  lifecycle::stop_runtime(&vm, &Default::default(), state).await?;
  super::process::delete_instances(&targets, state).await?;
  if inspect_runtime_id(&vm, state).await?.is_some() {
    return Err(IoError::interrupted(
      "VM runtime",
      "The runtime still exists after deletion",
    ));
  }
  cleanup_all_consoles(&vm, state).await?;
  VmDb::clear_by_pk(&vm.spec.vm_key, &state.inner.pool).await?;
  state
    .emit_normal_native_action_sync(&vm, NativeEventAction::Destroy)
    .await;
  Ok(())
}

/// Update the VM
///
pub async fn update(key: &str, state: &SystemState) -> IoResult<()> {
  let _guard = lifecycle_lock(key).lock_owned().await;
  let result = update_unlocked(key, state).await;
  record_task_failure(key, result, state).await
}

async fn update_unlocked(key: &str, state: &SystemState) -> IoResult<()> {
  let vm = VmDb::ensure_mac_address(key, state)
    .await
    .map_err(|err| IoError::invalid_input("VM MAC", &err.to_string()))?;
  // Fail before deleting the existing VM runtime.
  ensure_networks(&vm, state).await?;
  validate_spec(&vm.spec.clone().into())?;
  ensure_runtime_image(&vm, state).await?;
  prepare_console_directory(&vm, state).await?;
  let runtime_name = format!("{}.v", &vm.spec.vm_key);
  let old_processes =
    ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
      .await?;
  let mut old_runtime_keys = old_processes
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
  if let Some(id) = inspect_runtime_id(&vm, state).await? {
    if !old_runtime_keys.contains(&id) {
      old_runtime_keys.push(id);
    }
  }
  lifecycle::stop_runtime(&vm, &Default::default(), state).await?;
  super::process::delete_instances(&old_runtime_keys, state).await?;
  if let Err(err) = cleanup_process_consoles(&vm, &old_processes, state).await {
    log::warn!("Unable to clean previous VM console directory: {err}");
  }
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

  #[test]
  fn vm_custom_user_data_preserves_raw_and_renders_structured_config() {
    let raw = "#cloud-config\nwrite_files:\n  - content: 'é $HOME'\n\n";
    assert_eq!(cloud_init_user_data(None).unwrap(), None);
    for raw in [raw, ""] {
      assert_eq!(
        cloud_init_user_data(Some(&VmCloudInitUserData::Raw(raw.into())))
          .unwrap(),
        Some(raw.to_owned())
      );
    }
    let config = serde_json::json!({
      "users": [{"name": "custom", "lock_passwd": true}],
      "write_files": [{"path": "/tmp/example", "content": "yes\n$HOME\né"}],
      "runcmd": [["sh", "-c", "echo done"]]
    });
    let data: VmCloudInitUserData =
      serde_json::from_value(config.clone()).unwrap();
    let rendered = cloud_init_user_data(Some(&data)).unwrap().unwrap();
    assert!(rendered.starts_with("#cloud-config\n"));
    assert_eq!(
      serde_yaml::from_str::<serde_json::Value>(&rendered).unwrap(),
      config
    );
  }

  #[test]
  fn vm_custom_user_data_is_bounded_and_errors_hide_contents() {
    assert!(
      cloud_init_user_data(Some(&VmCloudInitUserData::Raw("é".repeat(32_768))))
        .is_ok()
    );
    for raw in ["é".repeat(32_769), "private-secret\0payload".to_owned()] {
      let error = cloud_init_user_data(Some(&VmCloudInitUserData::Raw(raw)))
        .unwrap_err()
        .to_string();
      assert!(error.contains("65536"));
      assert!(!error.contains("private-secret"));
    }
    let oversized: VmCloudInitUserData = serde_json::from_value(
      serde_json::json!({"write_files": [{"content": "x".repeat(65_536)}]}),
    )
    .unwrap();
    assert!(cloud_init_user_data(Some(&oversized)).is_err());
    let mut vm = Vm::default();
    vm.spec.cloud_init = Some(false);
    vm.spec.cloud_init_user_data = Some(oversized);
    assert!(validate_spec(&vm.spec.into()).is_err());
  }

  #[test]
  fn vm_custom_user_data_replaces_defaults_and_requires_runtime_support() {
    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    vm.spec.user = Some("ignored invalid user".into());
    vm.spec.password = Some("ignored password".into());
    vm.spec.ssh_key = Some("ignored key".into());
    vm.spec.cloud_init_user_data =
      Some(VmCloudInitUserData::Raw(String::new()));
    assert!(validate_spec(&vm.spec.clone().into()).is_ok());
    let env = runtime_env(&vm, false).unwrap();
    assert!(env.contains(&"CLOUD_INIT_USER_DATA=".into()));
    assert!(env.contains(&format!("INSTANCE_ID={}", vm.spec.vm_key)));
    for key in ["USER=", "PASSWORD=", "SSH_KEY="] {
      assert!(!env.iter().any(|value| value.starts_with(key)));
    }
    let mut labels = HashMap::from([
      ("io.nanocl.vm-runtime-api".into(), "0.19".into()),
      ("io.nanocl.vm-qmp".into(), "1".into()),
      ("io.nanocl.vm-resize".into(), "1".into()),
    ]);
    assert!(validate_runtime_labels(&vm, None).is_err());
    assert!(validate_runtime_labels(&vm, Some(&labels)).is_err());
    labels.insert("io.nanocl.vm-cloud-init-user-data".into(), "1".into());
    assert!(validate_runtime_labels(&vm, Some(&labels)).is_ok());
    labels.remove("io.nanocl.vm-resize");
    assert!(validate_runtime_labels(&vm, Some(&labels)).is_err());
    labels.insert("io.nanocl.vm-resize".into(), "1".into());
    labels.remove("io.nanocl.vm-cloud-init-user-data");
    vm.spec.cloud_init = Some(false);
    assert!(validate_runtime_labels(&vm, Some(&labels)).is_ok());
    assert!(
      !runtime_env(&vm, false)
        .unwrap()
        .iter()
        .any(|value| value.starts_with("CLOUD_INIT_USER_DATA="))
    );
    vm.spec.cloud_init = None;
    vm.spec.cloud_init_user_data = None;
    assert!(runtime_env(&vm, false).is_err());
    assert!(validate_runtime_labels(&vm, Some(&labels)).is_ok());
  }

  #[test]
  fn vm_runtime_link_interface_matches_docker_network_prefix() {
    assert!(validate_link_net_iface(None, None).is_ok());
    assert!(validate_link_net_iface(Some("eth0"), None).is_ok());
    assert!(validate_link_net_iface(Some("eth1"), None).is_err());
    let options = HashMap::from([(
      "com.docker.network.container_iface_prefix".to_owned(),
      "vm".to_owned(),
    )]);
    assert!(validate_link_net_iface(None, Some(&options)).is_ok());
    assert!(validate_link_net_iface(Some("vm0"), Some(&options)).is_ok());
    assert!(validate_link_net_iface(Some("eth0"), Some(&options)).is_err());

    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    assert!(
      !runtime_env(&vm, false)
        .unwrap()
        .iter()
        .any(|item| item.starts_with("DEFAULT_INTERFACE="))
    );
    vm.spec.host_config.link_net_iface = Some("vm0".into());
    assert!(
      runtime_env(&vm, false)
        .unwrap()
        .contains(&"DEFAULT_INTERFACE=vm0".into())
    );
  }

  #[test]
  fn vm_runtime_validates_user_only_when_cloud_init_is_enabled() {
    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    for user in [None, Some(""), Some("cloud"), Some("_guest-123")] {
      vm.spec.user = user.map(str::to_owned);
      assert!(validate_spec(&vm.spec.clone().into()).is_ok());
      assert!(runtime_env(&vm, false).is_ok());
    }
    for user in [
      "Alice",
      "1guest",
      "-guest",
      "guest.name",
      "guest name",
      "guést",
      "guest\nname",
    ] {
      vm.spec.user = Some(user.to_owned());
      for enabled in [None, Some(true)] {
        vm.spec.cloud_init = enabled;
        assert!(validate_spec(&vm.spec.clone().into()).is_err());
        assert!(runtime_env(&vm, false).is_err());
      }
      vm.spec.cloud_init = Some(false);
      assert!(validate_spec(&vm.spec.clone().into()).is_ok());
      assert!(
        !runtime_env(&vm, false)
          .unwrap()
          .iter()
          .any(|item| item.starts_with("USER="))
      );
    }
  }

  #[test]
  fn vm_generated_cloud_init_rejects_multiline_credentials_without_secrets() {
    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    for value in [None, Some(""), Some("normal credential: 'value'")] {
      vm.spec.password = value.map(str::to_owned);
      vm.spec.ssh_key = value.map(str::to_owned);
      assert!(validate_spec(&vm.spec.clone().into()).is_ok());
      assert!(runtime_env(&vm, false).is_ok());
    }
    for field in ["Password", "SshKey"] {
      for separator in ['\r', '\n'] {
        let secret = format!("private-credential{separator}must-not-leak");
        vm.spec.password = (field == "Password").then(|| secret.clone());
        vm.spec.ssh_key = (field == "SshKey").then(|| secret.clone());
        for enabled in [None, Some(true)] {
          vm.spec.cloud_init = enabled;
          for error in [
            validate_spec(&vm.spec.clone().into()).unwrap_err(),
            runtime_env(&vm, false).unwrap_err(),
          ] {
            assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
            assert!(error.to_string().contains(field));
            let diagnostics = format!("{error} {error:?}");
            assert!(!diagnostics.contains("private-credential"));
            assert!(!diagnostics.contains("must-not-leak"));
          }
        }
      }
    }
  }

  #[test]
  fn vm_ignored_cloud_init_credentials_allow_multiline_values() {
    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    vm.spec.password = Some("ignored\r\npassword".into());
    vm.spec.ssh_key = Some("ignored\r\nkey".into());
    for (enabled, data) in [
      (Some(false), None),
      (None, Some(VmCloudInitUserData::Raw(String::new()))),
      (Some(true), Some(VmCloudInitUserData::Raw(String::new()))),
    ] {
      vm.spec.cloud_init = enabled;
      vm.spec.cloud_init_user_data = data;
      assert!(validate_spec(&vm.spec.clone().into()).is_ok());
      let env = runtime_env(&vm, false).unwrap();
      assert!(!env.iter().any(|value| {
        value.starts_with("PASSWORD=") || value.starts_with("SSH_KEY=")
      }));
    }
  }

  #[test]
  fn vm_runtime_requires_isolated_network_and_persisted_identity() {
    for mode in ["host", "none", "", "default", "container:other"] {
      assert!(validate_network_mode(mode).is_err());
    }
    for mode in ["bridge", "nanoclbr0", "vm-test"] {
      assert!(validate_network_mode(mode).is_ok());
    }
    assert!(runtime_env(&Vm::default(), true).is_err());
    let mut vm = Vm::default();
    vm.spec.mac_address = Some("02:00:00:00:00:01".into());
    vm.spec.cloud_init = Some(false);
    vm.spec.console = Some(true);
    vm.spec.password = Some("must-not-pass".into());
    vm.spec.host_config.dns = Some(vec!["192.0.2.53".into()]);
    let env = runtime_env(&vm, false).unwrap();
    assert!(env.contains(&"MAC=02:00:00:00:00:01".into()));
    assert!(env.contains(&"CLOUD_INIT=false".into()));
    assert!(env.contains(&"CONSOLE=true".into()));
    assert!(env.contains(&"DNS=192.0.2.53".into()));
    assert!(!env.iter().any(|item| item.starts_with("PASSWORD=")));
    for invalid in ["1.1.1.1\nport=0", "169.254.254.1"] {
      vm.spec.host_config.dns = Some(vec![invalid.into()]);
      assert!(runtime_env(&vm, false).is_err());
    }
    vm.spec.host_config.dns = None;
    vm.spec.host_config.link_net_iface = Some("eth0.1".into());
    assert!(runtime_env(&vm, false).is_err());
  }

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
  fn vm_runtime_cleanup_requires_matching_owner_and_immutable_id() {
    let vm = Vm::default();
    let mut inspect = bollard_next::models::ContainerInspectResponse {
      id: Some("runtime-id".into()),
      name: Some(format!("/{}.v", vm.spec.vm_key)),
      config: Some(bollard_next::models::ContainerConfig {
        labels: Some(HashMap::from([
          ("io.nanocl.v".into(), vm.spec.vm_key.clone()),
          ("io.nanocl.n".into(), vm.namespace_name.clone()),
        ])),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert_eq!(owned_runtime_id(&vm, &inspect).unwrap(), "runtime-id");
    inspect
      .config
      .as_mut()
      .unwrap()
      .labels
      .as_mut()
      .unwrap()
      .insert("io.nanocl.v".into(), "other.vm".into());
    assert!(owned_runtime_id(&vm, &inspect).is_err());
  }
}
