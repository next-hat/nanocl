use std::collections::HashMap;

use futures::StreamExt;
use futures_util::stream::FuturesUnordered;

use bollard_next::{
  auth::DockerCredentials,
  container::{
    Config, CreateContainerOptions, InspectContainerOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
    WaitContainerOptions,
  },
  service::{DeviceMapping, HostConfig, RestartPolicy, RestartPolicyNameEnum},
};
use nanocl_error::{
  http::{HttpError, HttpResult},
  io::{FromIo, IoError, IoResult},
};

use nanocl_stubs::{
  cargo::{Cargo, CargoKillOptions},
  generic::{GenericClause, GenericFilter, ImagePullPolicy},
  job::Job,
  process::{Process, ProcessKind, ProcessPartial},
  system::{
    EventActor, EventActorKind, EventKind, EventPartial, NativeEventAction,
    ObjPsStatusKind,
  },
  vm::Vm,
};

use crate::{
  vars,
  models::{
    CargoDb, JobDb, JobUpdateDb, ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb,
    SecretDb, SystemState, VmDb, VmImageDb,
  },
  repositories::generic::*,
};

/// Get the image name and tag from a string
pub fn parse_img_name(name: &str) -> HttpResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
  if image_info.len() != 2 {
    return Err(HttpError::bad_request("Missing tag in image name"));
  }
  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// Download the image
pub async fn download_image<A>(
  image: &str,
  secret: Option<String>,
  policy: ImagePullPolicy,
  actor: &A,
  state: &SystemState,
) -> HttpResult<()>
where
  A: Into<EventActor> + Clone,
{
  match policy {
    ImagePullPolicy::Always => {}
    ImagePullPolicy::IfNotPresent => {
      if state.inner.docker_api.inspect_image(image).await.is_ok() {
        return Ok(());
      }
    }
    ImagePullPolicy::Never => {
      return Ok(());
    }
  }
  let credentials = match secret {
    Some(secret) => {
      let secret = SecretDb::read_by_pk(&secret, &state.inner.pool).await?;
      serde_json::from_value::<DockerCredentials>(secret.data)
        .map(Some)
        .map_err(|err| HttpError::bad_request(err.to_string()))?
    }
    None => None,
  };
  let (name, tag) = parse_img_name(image)?;
  let mut stream = state.inner.docker_api.create_image(
    Some(bollard_next::image::CreateImageOptions {
      from_image: name.clone(),
      tag: tag.clone(),
      ..Default::default()
    }),
    None,
    credentials,
  );
  while let Some(chunk) = stream.next().await {
    let chunk = match chunk {
      Err(err) => {
        let event = EventPartial {
          reporting_controller: vars::CONTROLLER_NAME.to_owned(),
          reporting_node: state.inner.config.hostname.clone(),
          action: NativeEventAction::Downloading.to_string(),
          reason: "state_sync".to_owned(),
          kind: EventKind::Error,
          actor: Some(EventActor {
            key: Some(image.to_owned()),
            kind: EventActorKind::ContainerImage,
            attributes: None,
          }),
          related: Some(actor.clone().into()),
          note: Some(format!("Error while downloading image {image} {err}")),
          metadata: None,
        };
        state.spawn_emit_event(event);
        return Err(err.into());
      }
      Ok(chunk) => chunk,
    };
    let event = EventPartial {
      reporting_controller: vars::CONTROLLER_NAME.to_owned(),
      reporting_node: state.inner.config.hostname.clone(),
      action: NativeEventAction::Downloading.to_string(),
      reason: "state_sync".to_owned(),
      kind: EventKind::Normal,
      actor: Some(EventActor {
        key: Some(image.to_owned()),
        kind: EventActorKind::ContainerImage,
        attributes: None,
      }),
      related: Some(actor.clone().into()),
      note: Some(format!("Downloading image {name}:{tag}")),
      metadata: Some(serde_json::json!({
        "state": chunk,
      })),
    };
    state.spawn_emit_event(event);
  }
  let event = EventPartial {
    reporting_controller: vars::CONTROLLER_NAME.to_owned(),
    reporting_node: state.inner.config.hostname.clone(),
    action: NativeEventAction::Download.to_string(),
    reason: "state_sync".to_owned(),
    kind: EventKind::Normal,
    actor: Some(EventActor {
      key: Some(image.to_owned()),
      kind: EventActorKind::ContainerImage,
      attributes: None,
    }),
    related: Some(actor.clone().into()),
    note: Some(format!("{name}:{tag}")),
    metadata: None,
  };
  state.spawn_emit_event(event);
  Ok(())
}

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
      state.emit_normal_native_action(&vm, action);
    }
    ProcessKind::Cargo => {
      let cargo =
        CargoDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
      state.emit_normal_native_action(&cargo, action);
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
      state.emit_normal_native_action(&job, action);
    }
  }
  Ok(())
}

/// Create a process (container) based on the kind and the item
pub async fn create_instance(
  kind: &ProcessKind,
  name: &str,
  kind_key: &str,
  item: &Config,
  state: &SystemState,
) -> HttpResult<Process> {
  let mut config = item.clone();
  let mut labels = item.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  labels.insert("io.nanocl.kind".to_owned(), kind.to_string());
  config.labels = Some(labels);
  let res = state
    .inner
    .docker_api
    .create_container(
      Some(CreateContainerOptions {
        name,
        ..Default::default()
      }),
      config,
    )
    .await?;
  let inspect = state
    .inner
    .docker_api
    .inspect_container(&res.id, None::<InspectContainerOptions>)
    .await?;
  let created_at = inspect.created.clone().unwrap_or_default();
  let new_instance = ProcessPartial {
    key: res.id,
    name: name.to_owned(),
    kind: kind.clone(),
    data: serde_json::to_value(&inspect)
      .map_err(|err| err.map_err_context(|| "CreateProcess"))?,
    node_name: state.inner.config.hostname.clone(),
    kind_key: kind_key.to_owned(),
    created_at: Some(
      chrono::NaiveDateTime::parse_from_str(
        &created_at,
        "%Y-%m-%dT%H:%M:%S%.fZ",
      )
      .map_err(|err| {
        HttpError::internal_server_error(format!("Unable to parse date {err}"))
      })?,
    ),
  };
  let process =
    ProcessDb::create_from(&new_instance, &state.inner.pool).await?;
  Process::try_from(process).map_err(HttpError::from)
}

/// Container to execute before the cargo instances
async fn execute_cargo_before(
  cargo: &Cargo,
  state: &SystemState,
) -> HttpResult<()> {
  match cargo.spec.init_container.clone() {
    Some(mut before) => {
      let image = before
        .image
        .clone()
        .unwrap_or(cargo.spec.container.image.clone().unwrap());
      before.image = Some(image.clone());
      before.host_config = Some(HostConfig {
        network_mode: Some(cargo.namespace_name.clone()),
        ..before.host_config.unwrap_or_default()
      });
      download_image(
        &image,
        cargo.spec.image_pull_secret.clone(),
        cargo.spec.image_pull_policy.clone().unwrap_or_default(),
        cargo,
        state,
      )
      .await?;
      let mut labels = before.labels.to_owned().unwrap_or_default();
      labels.insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
      labels.insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
      labels.insert("io.nanocl.init-c".to_owned(), "true".to_owned());
      labels.insert(
        "com.docker.compose.project".into(),
        format!("nanocl_{}", cargo.namespace_name),
      );
      before.labels = Some(labels);
      let short_id = super::key::generate_short_id(6);
      let name = format!(
        "init-{}-{}.{}.c",
        cargo.spec.name, short_id, cargo.namespace_name
      );
      create_instance(
        &ProcessKind::Cargo,
        &name,
        &cargo.spec.cargo_key,
        &before,
        state,
      )
      .await?;
      state
        .inner
        .docker_api
        .start_container(&name, None::<StartContainerOptions<String>>)
        .await?;
      let options = Some(WaitContainerOptions {
        condition: "not-running",
      });
      let mut stream = state.inner.docker_api.wait_container(&name, options);
      while let Some(wait_status) = stream.next().await {
        log::trace!("init_container: wait {wait_status:?}");
        match wait_status {
          Ok(wait_status) => {
            log::debug!("Wait status: {wait_status:?}");
            if wait_status.status_code != 0 {
              let error = match wait_status.error {
                Some(error) => error.message.unwrap_or("Unknown error".into()),
                None => "Unknown error".into(),
              };
              return Err(HttpError::internal_server_error(format!(
                "Error while waiting for before container: {error}"
              )));
            }
          }
          Err(err) => {
            return Err(HttpError::internal_server_error(format!(
              "Error while waiting for before container: {err}"
            )));
          }
        }
      }
      Ok(())
    }
    None => Ok(()),
  }
}

/// Create instances (containers) based on the cargo spec
/// The number of containers created is based on the number of instances defined in the cargo spec
/// Example: cargo-key-(random-id), cargo-key-(random-id1), cargo-key-(random-id2)
pub async fn create_cargo(
  cargo: &Cargo,
  number: usize,
  state: &SystemState,
) -> HttpResult<Vec<Process>> {
  execute_cargo_before(cargo, state).await?;
  download_image(
    &cargo.spec.container.image.clone().unwrap_or_default(),
    cargo.spec.image_pull_secret.clone(),
    cargo.spec.image_pull_policy.clone().unwrap_or_default(),
    cargo,
    state,
  )
  .await?;
  let mut secret_envs: Vec<String> = Vec::new();
  if let Some(secrets) = &cargo.spec.secrets {
    let filter = GenericFilter::new()
      .r#where("key", GenericClause::In(secrets.clone()))
      .r#where("kind", GenericClause::Eq("nanocl.io/env".to_owned()));
    let secrets = SecretDb::transform_read_by(&filter, &state.inner.pool)
      .await?
      .into_iter()
      .map(|secret| {
        let envs = serde_json::from_value::<Vec<String>>(secret.data)?;
        Ok::<_, IoError>(envs)
      })
      .collect::<IoResult<Vec<Vec<String>>>>()?;
    // Flatten the secrets to have envs in a single vector
    secret_envs = secrets.into_iter().flatten().collect();
  }
  (0..number)
    .collect::<Vec<usize>>()
    .into_iter()
    .map(move |current| {
      let secret_envs = secret_envs.clone();
      async move {
        let ordinal_index = if current > 0 {
          current.to_string()
        } else {
          "".to_owned()
        };
        let short_id = super::key::generate_short_id(6);
        let name = format!("{}-{}.{}.c", cargo.spec.name, short_id, cargo.namespace_name);
        let spec = cargo.spec.clone();
        let container = spec.container;
        let host_config = container.host_config.unwrap_or_default();
        // Add cargo label to the container to track it
        let mut labels = container.labels.to_owned().unwrap_or_default();
        labels.insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
        labels
          .insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
        labels.insert(
          "com.docker.compose.project".into(),
          format!("nanocl_{}", cargo.namespace_name),
        );
        let auto_remove =
          host_config
          .auto_remove
          .unwrap_or(false);
        if auto_remove {
          return Err(HttpError::bad_request("Using auto remove for a cargo is not allowed, consider using a job instead"));
        }
        let restart_policy =
          Some(
              host_config
              .restart_policy
              .unwrap_or(RestartPolicy {
                name: Some(RestartPolicyNameEnum::ALWAYS),
                maximum_retry_count: None,
              }),
          );
        let mut env = container.env.unwrap_or_default();
        // merge cargo env with secret env
        env.extend(secret_envs);
        env.push(format!("NANOCL_NODE={}", state.inner.config.hostname));
        env.push(format!("NANOCL_NODE_ADDR={}", state.inner.config.gateway));
        env.push(format!("NANOCL_CARGO_KEY={}", cargo.spec.cargo_key.to_owned()));
        env.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
        env.push(format!("NANOCL_CARGO_INSTANCE={}", current));
        // Merge the cargo spec with the container spec
        // And set his network mode to the cargo namespace
        let hostname = match &cargo.spec.container.hostname {
          None => format!("{}{}", ordinal_index, cargo.spec.name),
          Some(hostname) => format!("{}{}", ordinal_index, hostname),
        };
        let new_process = bollard_next::container::Config {
          attach_stderr: Some(true),
          attach_stdout: Some(true),
          tty: Some(true),
          hostname: Some(hostname),
          labels: Some(labels),
          env: Some(env),
          host_config: Some(HostConfig {
            restart_policy,
            network_mode: Some(
                host_config
                .network_mode
                .unwrap_or(cargo.namespace_name.clone()),
            ),
            ..host_config
          }),
          ..container
        };
        create_instance(
          &ProcessKind::Cargo,
          &name,
          &cargo.spec.cargo_key,
          &new_process,
          state,
        ).await
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<HttpResult<Process>>>()
    .await
    .into_iter()
    .collect::<HttpResult<Vec<Process>>>()
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
        if *status_code != 404 {
          return Err(err.into());
        }
      }
      _ => {
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
  let status = ObjPsStatusDb::read_by_pk(kind_pk, &state.inner.pool).await?;
  // If the process is already stopped, return
  if status.actual == ObjPsStatusKind::Stop.to_string() {
    return Ok(());
  }
  let processes =
    ProcessDb::read_by_kind_key(kind_pk, &state.inner.pool).await?;
  log::debug!("stop_process_by_kind_pk: {kind_pk}");
  for process in processes {
    let process_state = process.data.state.unwrap_or_default();
    if !process_state.running.unwrap_or_default() {
      return Ok(());
    }
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
  let status = ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  // If the process is already running, return
  if status.actual == ObjPsStatusKind::Start.to_string() {
    return Ok(());
  }
  let processes =
    ProcessDb::read_by_kind_key(kind_key, &state.inner.pool).await?;
  for process in processes {
    let process_state = process.data.state.unwrap_or_default();
    if process_state.running.unwrap_or_default() {
      continue;
    }
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
  download_image(&image, None, ImagePullPolicy::IfNotPresent, vm, state)
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
          .unwrap_or(vm.namespace_name.to_owned()),
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
    create_instance(&ProcessKind::Vm, &name, &vm.spec.vm_key, &spec, state)
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
  let short_id = super::key::generate_short_id(6);
  let container_name = format!("{name}-{index}-{short_id}.j");
  create_instance(&ProcessKind::Job, &container_name, name, &container, state)
    .await
}

/// Create processes (container) for a job
pub async fn create_job_instances(
  job: &Job,
  state: &SystemState,
) -> HttpResult<Vec<Process>> {
  let mut processes = Vec::new();
  for (index, container) in job.containers.iter().enumerate() {
    download_image(
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
  if ProcessKind::Cargo == *kind && current_status.actual == wanted {
    log::debug!("{kind:?} {kind_key} already running",);
    return Ok(());
  }
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
