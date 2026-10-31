use std::{
  io::ErrorKind, os::unix::fs::PermissionsExt, path::Path, sync::Arc,
  time::Duration,
};

use bollard_next::models::ContainerInspectResponse;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  system::{NativeEventAction, ObjPsStatusKind},
  vm::{Vm, VmResizeOptions, VmResources},
  vm_spec::{VmHostConfig, VmSpec, VmSpecPartial},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  time::Instant,
};

use super::lifecycle;
use crate::{
  models::{
    QmpBlockNode, QmpClient, QmpMemoryDevice, SystemState, VmDb,
    VmLifecycleGuard, VmRuntimeResources,
  },
  repositories::generic::*,
};

const MIB: u64 = 1024 * 1024;
const MAX_MEMORY: u64 = i64::MAX as u64 / MIB;

fn conflict(message: &str) -> IoError {
  IoError::with_context(
    "VM resize",
    std::io::Error::new(ErrorKind::WouldBlock, message),
  )
}

pub(super) fn configured_resources(
  config: &VmHostConfig,
) -> IoResult<VmRuntimeResources> {
  // Preserve the existing zero-as-default boot behavior.
  let cpu = if config.cpu == 0 { 1 } else { config.cpu };
  let memory = if config.memory == 0 {
    512
  } else {
    config.memory
  };
  let resources = VmRuntimeResources {
    cpu,
    memory,
    max_cpu: config.max_cpu.unwrap_or(cpu),
    max_memory: config.max_memory.unwrap_or(memory),
  };
  validate_resources(&resources)?;
  Ok(resources)
}

fn validate_resources(value: &VmRuntimeResources) -> IoResult<()> {
  if value.cpu == 0
    || value.cpu > value.max_cpu
    || value.max_cpu > 255
    || value.memory == 0
    || value.memory > value.max_memory
    || value.max_memory > MAX_MEMORY
  {
    return Err(IoError::invalid_input(
      "VM resources",
      "Cpu must be 1..MaxCpu (at most 255), and Memory must be positive MiB up to MaxMemory (at most 8796093022207 MiB)",
    ));
  }
  Ok(())
}

async fn private_directory(directory: &Path) -> IoResult<()> {
  let metadata = tokio::fs::symlink_metadata(directory).await?;
  if !metadata.is_dir()
    || metadata.permissions().mode() & 0o077 != 0
    || tokio::fs::canonicalize(directory).await? != directory
  {
    return Err(conflict(
      "VM resource directory must be canonical and private",
    ));
  }
  Ok(())
}

async fn read_resources(directory: &Path) -> IoResult<VmRuntimeResources> {
  private_directory(directory).await?;
  let file = tokio::fs::OpenOptions::new()
    .read(true)
    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
    .open(directory.join("resources.json"))
    .await?;
  let metadata = file.metadata().await?;
  if !metadata.is_file()
    || metadata.len() > 4096
    || metadata.permissions().mode() & 0o077 != 0
  {
    return Err(conflict(
      "VM restart resources must be a private regular file of at most 4096 bytes",
    ));
  }
  let mut bytes = Vec::new();
  file.take(4097).read_to_end(&mut bytes).await?;
  if bytes.len() > 4096 {
    return Err(conflict("VM restart resources exceed 4096 bytes"));
  }
  let resources: VmRuntimeResources = serde_json::from_slice(&bytes)
    .map_err(|_| conflict("VM restart resources are invalid"))?;
  validate_resources(&resources)?;
  Ok(resources)
}

pub(super) async fn write_resources(
  directory: &Path,
  resources: &VmRuntimeResources,
) -> IoResult<()> {
  validate_resources(resources)?;
  private_directory(directory).await?;
  let temporary =
    directory.join(format!("resources-{}.tmp", uuid::Uuid::new_v4()));
  let result = async {
    let mut file = tokio::fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .mode(0o600)
      .open(&temporary)
      .await?;
    file.write_all(&serde_json::to_vec(resources)?).await?;
    file.sync_all().await?;
    tokio::fs::rename(&temporary, directory.join("resources.json")).await?;
    tokio::fs::File::open(directory).await?.sync_all().await?;
    Ok::<_, IoError>(())
  }
  .await;
  if result.is_err() {
    let _ = tokio::fs::remove_file(temporary).await;
  }
  result
}

fn same_boot_spec(current: &VmSpec, boot: &VmSpec) -> bool {
  if current.vm_key != boot.vm_key {
    return false;
  }
  let mut current: VmSpecPartial = current.clone().into();
  let mut boot: VmSpecPartial = boot.clone().into();
  for spec in [&mut current, &mut boot] {
    let config = spec
      .host_config
      .as_mut()
      .expect("VmSpec always has HostConfig");
    config.cpu = 0;
    config.memory = 0;
    if let Some(init) = spec.init_container.as_mut() {
      // Docker encodes these as maps, so key order is not a boot change.
      for keys in [&mut init.exposed_ports, &mut init.volumes]
        .into_iter()
        .flatten()
      {
        keys.sort_unstable();
      }
    }
  }
  current == boot
}

/// Resource-only history entries retain the immutable runtime boot generation.
/// Other desired changes must go through the normal replacement lifecycle.
pub(crate) async fn validate_boot_spec(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  state: &SystemState,
) -> IoResult<VmSpec> {
  super::owned_runtime_id(vm, runtime)?;
  let generation = runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref())
    .and_then(|labels| labels.get("io.nanocl.vm-spec"))
    .and_then(|value| uuid::Uuid::parse_str(value).ok())
    .ok_or_else(|| conflict("VM runtime generation is invalid"))?;
  if generation == vm.spec.key {
    return Ok(vm.spec.clone());
  }
  let spec = super::validate_generation(vm, generation, state).await?;
  let boot = spec.try_to_vm_spec()?;
  if !same_boot_spec(&vm.spec, &boot) {
    return Err(conflict("VM has a pending replacement specification"));
  }
  Ok(boot)
}

fn validate_disk_mount(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
) -> IoResult<()> {
  if !Path::new(&vm.spec.image).is_absolute() {
    return Err(conflict("VM disk path must be absolute"));
  }
  let mut mounts =
    runtime
      .mounts
      .as_deref()
      .unwrap_or_default()
      .iter()
      .filter(|mount| {
        mount.destination.as_deref() == Some(vm.spec.image.as_str())
      });
  let mount = mounts
    .next()
    .ok_or_else(|| conflict("VM disk bind mount is missing"))?;
  if mounts.next().is_some()
    || mount.source.as_deref() != Some(vm.spec.image.as_str())
    || mount.rw != Some(true)
    || mount.typ.as_deref() != Some("bind")
  {
    return Err(conflict(
      "VM disk bind mount does not match its specification",
    ));
  }
  Ok(())
}

async fn disk(client: &mut QmpClient, image: &str) -> IoResult<QmpBlockNode> {
  let mut matches = client
    .query_block_devices()
    .await?
    .into_iter()
    .filter(|device| {
      !device.removable && device.device == "ide0-hd0" && device.qdev.is_some()
    })
    .filter_map(|device| device.inserted)
    .filter(|node| node.file == image && node.image.filename == image);
  let node = matches
    .next()
    .ok_or_else(|| conflict("QEMU does not expose the expected VM disk"))?;
  if matches.next().is_some() {
    return Err(conflict("VM disk identity is ambiguous"));
  }
  Ok(node)
}

fn validate_disk_growth(node: &QmpBlockNode, target: u64) -> IoResult<()> {
  if target < node.image.virtual_size
    || target > i64::MAX as u64
    || target % 512 != 0
  {
    return Err(IoError::invalid_input(
      "VM disk resize",
      "DiskSize must not shrink the disk and must be a positive multiple of 512 bytes within i64::MAX",
    ));
  }
  super::super::qmp::validate_disk_growth(node, target)
}

async fn allocation(
  client: &mut QmpClient,
  maxima: &VmRuntimeResources,
) -> IoResult<VmRuntimeResources> {
  let cpu = client.query_cpus_count().await?;
  let memory = client.query_memory_size().await?;
  let bytes = memory
    .base_memory
    .checked_add(memory.plugged_memory.unwrap_or(0))
    .ok_or_else(|| conflict("QEMU memory total overflow"))?;
  if bytes % MIB != 0 {
    return Err(conflict("QEMU memory is not an integral MiB allocation"));
  }
  let resources = VmRuntimeResources {
    cpu,
    memory: bytes / MIB,
    ..maxima.clone()
  };
  validate_resources(&resources)?;
  Ok(resources)
}

fn targets(
  current: &VmRuntimeResources,
  options: &VmResizeOptions,
) -> IoResult<VmRuntimeResources> {
  let target = VmRuntimeResources {
    cpu: options.cpu.unwrap_or(current.cpu),
    memory: options.memory.unwrap_or(current.memory),
    ..current.clone()
  };
  validate_resources(&target)?;
  if target.cpu < current.cpu || target.memory < current.memory {
    return Err(IoError::invalid_input(
      "VM resize",
      "Live CPU and memory shrinking is not supported",
    ));
  }
  if (target.memory - current.memory) % 128 != 0 {
    return Err(IoError::invalid_input(
      "VM resize",
      "Live memory increases must be multiples of 128 MiB",
    ));
  }
  Ok(target)
}

fn memory_slots(devices: &[QmpMemoryDevice]) -> IoResult<usize> {
  let mut slots = std::collections::BTreeSet::new();
  for device in devices {
    if device.kind != "dimm"
      || device.data.size == 0
      || device.data.size % MIB != 0
      || !device
        .data
        .id
        .as_deref()
        .is_some_and(|id| id.starts_with("nanocl-dimm-"))
      || !device.data.memdev.starts_with("/objects/nanocl-mem-")
      || !device
        .data
        .slot
        .is_some_and(|slot| slot < 32 && slots.insert(slot))
    {
      return Err(conflict(
        "QEMU memory inventory contains unmanaged or invalid devices",
      ));
    }
  }
  Ok(slots.len())
}

async fn ready(
  key: &str,
  state: &SystemState,
) -> IoResult<(Vm, ContainerInspectResponse, VmRuntimeResources)> {
  let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
  if crate::models::OperationDb::current("Vm", key, &state.inner.pool)
    .await?
    .is_some_and(|operation| operation.outcome.is_none())
  {
    return Err(conflict(
      "A VM lifecycle operation is awaiting completion or recovery",
    ));
  }
  if vm.status.actual != ObjPsStatusKind::Start
    || vm.status.wanted != ObjPsStatusKind::Start
  {
    return Err(conflict(
      "Live resources require a running VM with no pending lifecycle transition",
    ));
  }
  let runtime = lifecycle::inspect_runtime(&vm, state)
    .await?
    .ok_or_else(|| conflict("VM runtime is missing"))?;
  if runtime.state.as_ref().and_then(|state| state.running) != Some(true) {
    return Err(conflict("VM runtime is not running"));
  }
  if runtime
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref())
    .and_then(|labels| labels.get("io.nanocl.vm-resize"))
    .map(String::as_str)
    != Some("1")
  {
    return Err(conflict(
      "This VM needs a planned recreation with a resize-capable runtime",
    ));
  }
  let boot = validate_boot_spec(&vm, &runtime, state).await?;
  validate_disk_mount(&vm, &runtime)?;
  let image = Path::new(&vm.spec.image);
  if !tokio::fs::symlink_metadata(image).await?.is_file()
    || tokio::fs::canonicalize(image).await? != image
  {
    return Err(conflict(
      "Live resizing requires a canonical regular VM disk file",
    ));
  }
  let maxima = configured_resources(&boot.host_config)?;
  Ok((vm, runtime, maxima))
}

pub(crate) async fn runtime_resources(
  key: &str,
  state: &SystemState,
) -> IoResult<VmResources> {
  let (vm, runtime, maxima) = ready(key, state).await?;
  let mut client = lifecycle::connect(
    &vm,
    &runtime,
    state,
    Instant::now() + Duration::from_secs(5),
  )
  .await?;
  let current = allocation(&mut client, &maxima).await?;
  let disk_size = disk(&mut client, &vm.spec.image).await?.image.virtual_size;
  let directory =
    lifecycle::socket_directory(&vm, &runtime, &state.inner.config.state_dir)?;
  let persistent = read_resources(&directory)
    .await
    .is_ok_and(|saved| saved == current)
    && configured_resources(&vm.spec.host_config)? == current;
  Ok(VmResources {
    cpu: current.cpu,
    memory: current.memory,
    max_cpu: current.max_cpu,
    max_memory: current.max_memory,
    disk_size,
    persistent,
  })
}

async fn persist(
  vm: &Vm,
  runtime: &ContainerInspectResponse,
  current: &VmRuntimeResources,
  guard: Arc<VmLifecycleGuard>,
  state: &SystemState,
) -> IoResult<()> {
  let directory =
    lifecycle::socket_directory(vm, runtime, &state.inner.config.state_dir)?;
  // File first: a DB outage must not make a successful hot-add disappear at
  // container restart. A same-target retry repairs a missing history revision.
  write_resources(&directory, current).await?;
  if configured_resources(&vm.spec.host_config)? != *current {
    let mut spec: VmSpecPartial = vm.spec.clone().into();
    let config = spec.host_config.as_mut().expect("VmSpec has HostConfig");
    config.cpu = current.cpu;
    config.memory = current.memory;
    VmDb::update_from_spec_owned(
      &vm.spec.vm_key,
      &spec,
      &vm.spec.version,
      guard,
      &state.inner.pool,
    )
    .await?;
  }
  Ok(())
}

async fn run_resize(
  key: &str,
  options: &VmResizeOptions,
  guard: Arc<VmLifecycleGuard>,
  state: &SystemState,
) -> IoResult<VmResources> {
  let (vm, runtime, maxima) = ready(key, state).await?;
  let deadline = Instant::now() + Duration::from_secs(60);
  let mut client = lifecycle::connect(&vm, &runtime, state, deadline).await?;
  if !client.query_status().await?.running {
    return Err(conflict("QEMU must be running to resize"));
  }
  let current = allocation(&mut client, &maxima).await?;
  let target = targets(&current, options)?;
  let block = disk(&mut client, &vm.spec.image).await?;
  if let Some(size) = options.disk_size {
    validate_disk_growth(&block, size)?;
  }
  // Verify all capacity before the first irreversible mutation.
  let mut slots = client.query_cpu_slots().await?;
  slots.retain(|slot| slot.qom_path.is_none() && slot.vcpus_count == 1);
  slots.sort_by(|left, right| left.props.cmp(&right.props));
  if (slots.len() as u64) < target.cpu - current.cpu {
    return Err(conflict(
      "QEMU has insufficient CPU slots; recreate with the requested MaxCpu",
    ));
  }
  let devices = client.query_memory_devices().await?;
  if target.memory > current.memory && memory_slots(&devices)? >= 32 {
    return Err(conflict(
      "All 32 memory hotplug slots are occupied; restart before growing memory again",
    ));
  }
  let directory =
    lifecycle::socket_directory(&vm, &runtime, &state.inner.config.state_dir)?;
  let saved = read_resources(&directory).await?;
  if saved.max_cpu != maxima.max_cpu || saved.max_memory != maxima.max_memory {
    return Err(conflict(
      "Runtime restart limits do not match its boot specification",
    ));
  }
  let operation = uuid::Uuid::new_v4().simple().to_string();
  let backend = format!("nanocl-mem-{operation}");
  let dimm = format!("nanocl-dimm-{operation}");
  let mut backend_attempted = false;
  let result = async {
    for (index, slot) in slots
      .iter()
      .take((target.cpu - current.cpu) as usize)
      .enumerate()
    {
      client
        .add_cpu(&format!("nanocl-cpu-{operation}-{index}"), slot)
        .await?;
    }
    if target.memory > current.memory {
      backend_attempted = true;
      client
        .add_memory_backend(&backend, (target.memory - current.memory) * MIB)
        .await?;
      client.add_memory_device(&dimm, &backend).await?;
    }
    if let Some(size) = options
      .disk_size
      .filter(|size| *size > block.image.virtual_size)
    {
      client.resize_block(&block.node_name, size).await?;
    }
    let observed = allocation(&mut client, &maxima).await?;
    if observed != target {
      return Err(conflict(
        "QEMU did not confirm the requested CPU/RAM allocation",
      ));
    }
    let disk_size = disk(&mut client, &vm.spec.image).await?.image.virtual_size;
    if options.disk_size.is_some_and(|size| size != disk_size) {
      return Err(conflict("QEMU did not confirm the requested disk capacity"));
    }
    Ok::<_, IoError>((observed, disk_size))
  }
  .await;
  // QEMU's socket chardev serves one active client. Release even a poisoned
  // session before opening the recovery connection.
  drop(client);
  match result {
    Ok((observed, disk_size)) => {
      persist(&vm, &runtime, &observed, guard.clone(), state).await.map_err(|_| conflict("Resources changed but persistence failed; inspect resources and retry the same targets"))?;
      state
        .emit_normal_native_action_sync(&vm, NativeEventAction::Update)
        .await;
      Ok(VmResources {
        cpu: observed.cpu,
        memory: observed.memory,
        max_cpu: observed.max_cpu,
        max_memory: observed.max_memory,
        disk_size,
        persistent: true,
      })
    }
    Err(error) => {
      // A command may have executed before its connection failed. Never repeat
      // it automatically or roll back by shrinking. Observe through a fresh
      // bounded session, persist any completed subset, then report the failure.
      let recovery = async {
        let mut client = lifecycle::connect(
          &vm,
          &runtime,
          state,
          Instant::now() + Duration::from_secs(5),
        )
        .await?;
        let observed = allocation(&mut client, &maxima).await?;
        let cleanup = async {
          if backend_attempted {
            let devices = client.query_memory_devices().await?;
            if !devices
              .iter()
              .any(|device| device.data.memdev == format!("/objects/{backend}"))
            {
              // Only this operation's unique, confirmed-unattached backend.
              let _ = client.delete_memory_backend(&backend).await;
            }
          }
          Ok::<_, IoError>(())
        }
        .await;
        drop(client);
        // Attempt persistence even if cleanup could not be confirmed; neither
        // a slow DB nor a DB error should prevent the earlier bounded cleanup.
        persist(&vm, &runtime, &observed, guard.clone(), state).await?;
        cleanup?;
        Ok::<_, IoError>(())
      }
      .await;
      let message = if recovery.is_ok() {
        "Live resize did not fully complete; confirmed CPU/RAM changes were saved. Inspect resources before retrying; earlier changes were not rolled back"
      } else {
        "Live resize outcome or persistence is uncertain. Inspect resources and retry the same targets before restarting; earlier changes were not rolled back"
      };
      state.emit_error_native_action(
        &vm,
        NativeEventAction::Update,
        Some(message.into()),
      );
      Err(IoError::with_context(
        "VM resize",
        std::io::Error::new(error.inner.kind(), message),
      ))
    }
  }
}

pub(crate) async fn resize(
  key: &str,
  options: &VmResizeOptions,
  state: &SystemState,
) -> IoResult<VmResources> {
  options
    .validate()
    .map_err(|error| IoError::invalid_input("VM resize", &error))?;
  let guard = Arc::new(super::lifecycle_lock(key).try_lock_owned().await?);
  let key = key.to_owned();
  let options = options.clone();
  let state = state.clone();
  let (tx, rx) = tokio::sync::oneshot::channel();
  // Client disconnection must not interrupt mutation reconciliation/persistence.
  ntex::rt::spawn(async move {
    let result = run_resize(&key, &options, guard.clone(), &state).await;
    drop(guard);
    let _ = tx.send(result);
  });
  rx.await.map_err(|_| {
    IoError::interrupted(
      "VM resize",
      "Resize task was interrupted; inspect resources before retrying",
    )
  })?
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resource_limits_and_grow_only_targets_are_checked_before_mutation() {
    let default = configured_resources(&VmHostConfig::default()).unwrap();
    assert_eq!((default.max_cpu, default.max_memory), (1, 512));
    let config = VmHostConfig {
      cpu: 2,
      memory: 512,
      max_cpu: Some(4),
      max_memory: Some(2048),
      ..Default::default()
    };
    let current = configured_resources(&config).unwrap();
    assert!(
      targets(
        &current,
        &VmResizeOptions {
          cpu: Some(4),
          memory: Some(640),
          disk_size: None
        }
      )
      .is_ok()
    );
    for (cpu, memory) in [(1, 512), (5, 512), (2, 511), (2, 513), (2, 2176)] {
      assert!(
        targets(
          &current,
          &VmResizeOptions {
            cpu: Some(cpu),
            memory: Some(memory),
            disk_size: None
          }
        )
        .is_err()
      );
    }
    assert!(
      targets(
        &current,
        &VmResizeOptions {
          cpu: Some(2),
          memory: Some(512),
          disk_size: None
        }
      )
      .is_ok()
    );
    assert!(
      configured_resources(&VmHostConfig {
        max_cpu: Some(256),
        ..config.clone()
      })
      .is_err()
    );
    assert!(
      configured_resources(&VmHostConfig {
        max_memory: Some(u64::MAX),
        ..config
      })
      .is_err()
    );
  }

  #[test]
  fn boot_identity_allows_only_cpu_memory_history_changes() {
    let mut boot = VmSpec::default();
    boot.vm_key = "global.test".into();
    boot.host_config.max_cpu = Some(4);
    boot.host_config.max_memory = Some(4096);
    let mut current = boot.clone();
    current.key = uuid::Uuid::new_v4();
    current.host_config.cpu = 4;
    current.host_config.memory = 2048;
    assert!(same_boot_spec(&current, &boot));
    current.host_config.max_cpu = Some(8);
    assert!(!same_boot_spec(&current, &boot));
    current = boot.clone();
    current.image = "/other.qcow2".into();
    assert!(!same_boot_spec(&current, &boot));
    current = boot.clone();
    current.cloud_init_user_data = Some(
      nanocl_stubs::vm_spec::VmCloudInitUserData::Raw("secret".into()),
    );
    assert!(!same_boot_spec(&current, &boot));
    current = boot.clone();
    current.vm_key = "other.test".into();
    assert!(!same_boot_spec(&current, &boot));
  }

  #[test]
  fn boot_identity_compares_init_container_maps_without_key_order() {
    let mut boot = VmSpec::default();
    boot.vm_key = "global.test".into();
    boot.init_container = Some(bollard_next::models::ContainerCreateBody {
      exposed_ports: Some(vec!["80/tcp".into(), "443/tcp".into()]),
      volumes: Some(vec!["/data".into(), "/cache".into()]),
      ..Default::default()
    });
    let mut current = boot.clone();
    current.key = uuid::Uuid::new_v4();
    current.host_config.cpu = 4;
    current.host_config.memory = 2048;
    let init = current.init_container.as_mut().unwrap();
    init.exposed_ports.as_mut().unwrap().reverse();
    init.volumes.as_mut().unwrap().reverse();
    assert!(same_boot_spec(&current, &boot));

    let equivalent = current.clone();
    current.init_container.as_mut().unwrap().exposed_ports =
      Some(vec!["80/tcp".into(), "8443/tcp".into()]);
    assert!(!same_boot_spec(&current, &boot));
    current = equivalent;
    current.init_container.as_mut().unwrap().volumes =
      Some(vec!["/data".into(), "/other".into()]);
    assert!(!same_boot_spec(&current, &boot));
  }

  #[test]
  fn live_disk_requires_the_exact_writable_bind() {
    let mut vm = Vm::default();
    vm.spec.image = "/state/vm.qcow2".into();
    let runtime: ContainerInspectResponse = serde_json::from_value(serde_json::json!({
      "Mounts": [{"Type":"bind", "Source":"/state/vm.qcow2", "Destination":"/state/vm.qcow2", "RW":true}]
    })).unwrap();
    assert!(validate_disk_mount(&vm, &runtime).is_ok());
    let mut foreign = runtime.clone();
    foreign.mounts.as_mut().unwrap()[0].source =
      Some("/state/other.qcow2".into());
    assert!(validate_disk_mount(&vm, &foreign).is_err());
    let mut readonly = runtime.clone();
    readonly.mounts.as_mut().unwrap()[0].rw = Some(false);
    assert!(validate_disk_mount(&vm, &readonly).is_err());
    for typ in [None, Some("volume"), Some("future-type")] {
      let mut changed = runtime.clone();
      changed.mounts.as_mut().unwrap()[0].typ = typ.map(str::to_owned);
      assert!(validate_disk_mount(&vm, &changed).is_err());
    }
    let mut duplicate = runtime.clone();
    duplicate
      .mounts
      .as_mut()
      .unwrap()
      .push(runtime.mounts.unwrap()[0].clone());
    assert!(validate_disk_mount(&vm, &duplicate).is_err());
  }

  #[test]
  fn memory_inventory_rejects_foreign_and_duplicate_slots() {
    let device: QmpMemoryDevice = serde_json::from_value(serde_json::json!({
      "type":"dimm", "data":{"id":"nanocl-dimm-one","size":134217728,
      "slot":0,"memdev":"/objects/nanocl-mem-one"}
    }))
    .unwrap();
    assert_eq!(memory_slots(std::slice::from_ref(&device)).unwrap(), 1);
    assert!(memory_slots(&[device.clone(), device.clone()]).is_err());
    let mut foreign = device.clone();
    foreign.data.memdev = "/objects/foreign".into();
    assert!(memory_slots(&[foreign]).is_err());
    let mut invalid = device;
    invalid.data.slot = Some(32);
    assert!(memory_slots(&[invalid]).is_err());
  }
}
