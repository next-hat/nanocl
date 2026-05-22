use futures::{StreamExt, stream::FuturesUnordered};
use nanocld_client::{ConnectOpts, NanocldClient};
use ntex::rt;
use std::time::Duration;

use bollard_next::{
  container::{
    Config, RemoveContainerOptions, RenameContainerOptions,
    StartContainerOptions, StopContainerOptions, WaitContainerOptions,
  },
  secret::{HostConfig, RestartPolicy, RestartPolicyNameEnum},
};
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  generic::{GenericClause, GenericFilter},
  node::DeleteNodeCargoParams,
  process::{Process, ProcessKind},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{CargoDb, NodeDb, ObjPsStatusDb, ProcessDb, SystemState},
  repositories::generic::*,
  utils,
};

fn create_cargo_env(
  cargo: &Cargo,
  secret_envs: Vec<String>,
  current: usize,
  state: &SystemState,
) -> Vec<String> {
  let mut envs = cargo.spec.container.env.clone().unwrap_or_default();
  // merge cargo env with secret env
  envs.extend(secret_envs);
  envs.push(format!("NANOCL_NODE={}", state.inner.config.hostname));
  envs.push(format!("NANOCL_NODE_ADDR={}", state.inner.config.gateway));
  envs.push(format!("NANOCL_CARGO_KEY={}", cargo.spec.cargo_key));
  envs.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
  envs.push(format!("NANOCL_CARGO_INSTANCE={}", current));
  envs
}

pub fn has_container_healthcheck(cargo: &Cargo, instances: &[Process]) -> bool {
  if cargo.spec.container.healthcheck.is_some() {
    return true;
  }
  instances.iter().any(|instance| {
    instance
      .data
      .config
      .as_ref()
      .and_then(|config| config.healthcheck.as_ref())
      .is_some()
      || instance
        .data
        .state
        .as_ref()
        .and_then(|container_state| container_state.health.as_ref())
        .is_some()
  })
}

async fn rename_instances_back(
  processes: &[Process],
  state: &SystemState,
) -> IoResult<()> {
  processes
    .iter()
    .map(|process| {
      let docker_api = state.inner.docker_api.clone();
      async move {
        docker_api
          .rename_container(
            &process.key,
            RenameContainerOptions {
              name: &process.name,
            },
          )
          .await
          .map_err(|err| err.map_err_context(|| "RenameContainer"))?;
        Ok::<_, IoError>(())
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<IoResult<Vec<_>>>()?;
  Ok(())
}

/// Function that create the init container of the cargo
///
async fn create_init_container(
  cargo: &Cargo,
  init_container: &Config,
  state: &SystemState,
) -> IoResult<Process> {
  let mut init_container = init_container.clone();
  let image = init_container
    .image
    .clone()
    .unwrap_or(cargo.spec.container.image.clone().unwrap());
  let host_config = init_container.host_config.unwrap_or_default();
  init_container.image = Some(image.clone());
  let secret_dir = utils::secret::create_tls_secrets(
    &cargo.spec.cargo_key,
    &ProcessKind::Cargo,
    &cargo.spec.secrets,
    state,
  )
  .await?;
  // Add the secret directory to the bind mounts
  let mut binds = host_config.binds.unwrap_or_default();
  binds.push(format!("{}:/opt/nanocl.io/secrets", secret_dir));
  init_container.host_config = Some(HostConfig {
    binds: Some(binds),
    network_mode: Some(
      host_config.network_mode.unwrap_or("nanoclbr0".to_owned()),
    ),
    ..host_config
  });
  super::image::download(
    &image,
    cargo.spec.image_pull_secret.clone(),
    cargo.spec.image_pull_policy.clone().unwrap_or_default(),
    cargo,
    state,
  )
  .await?;
  let env_secrets =
    utils::secret::load_env_secrets(&cargo.spec.secrets, state).await?;
  let env = create_cargo_env(cargo, env_secrets, 0, state);
  init_container.env = Some(env);
  let mut labels = init_container.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
  labels.insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
  labels.insert("io.nanocl.init-c".to_owned(), "true".to_owned());
  labels.insert(
    "com.docker.compose.project".into(),
    format!("nanocl_{}", cargo.namespace_name),
  );
  init_container.labels = Some(labels);
  let short_id = utils::key::generate_short_id(6);
  let name = format!(
    "init-{}-{}.{}.c",
    cargo.spec.name, short_id, cargo.namespace_name
  );
  let process = super::process::create(
    &ProcessKind::Cargo,
    &name,
    &cargo.spec.cargo_key,
    &init_container,
    state,
  )
  .await?;
  Ok(process)
}

/// Function that start and wait the status of the init container before the main cargo container
///
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
    log::trace!("init_container: wait {wait_status:?}");
    match wait_status {
      Ok(wait_status) => {
        log::debug!("Wait status: {wait_status:?}");
        if wait_status.status_code != 0 {
          let error = match wait_status.error {
            Some(error) => error.message.unwrap_or("Unknown error".to_owned()),
            None => "Unknown error".to_owned(),
          };
          return Err(IoError::interrupted(
            "InitContainer",
            &format!("{error} {}", wait_status.status_code),
          ));
        }
      }
      Err(err) => {
        return Err(IoError::interrupted("InitContainer", &format!("{err}")));
      }
    }
  }
  Ok(())
}

/// Execute the cargo spec to create the cargo container
///
pub async fn create(
  cargo: &Cargo,
  number: usize,
  state: &SystemState,
) -> IoResult<Vec<Process>> {
  let data = serde_json::to_string(&cargo)?;
  let new_data = super::generic::inject_data(&data, state).await?;
  let cargo = &serde_json::from_str::<Cargo>(&new_data)?;
  super::image::download(
    &cargo.spec.container.image.clone().unwrap_or_default(),
    cargo.spec.image_pull_secret.clone(),
    cargo.spec.image_pull_policy.clone().unwrap_or_default(),
    cargo,
    state,
  )
  .await?;
  let env_secrets =
    utils::secret::load_env_secrets(&cargo.spec.secrets, state).await?;
  let secret_dir = utils::secret::create_tls_secrets(
    &cargo.spec.cargo_key,
    &ProcessKind::Cargo,
    &cargo.spec.secrets,
    state,
  )
  .await?;
  let instances = (0..number)
    .collect::<Vec<usize>>()
    .into_iter()
    .map(move |current| {
      let env_secrets = env_secrets.clone();
      let secret_dir = secret_dir.clone();
      async move {
        let ordinal_index = if current > 0 {
          current.to_string()
        } else {
          "".to_owned()
        };
        let short_id = utils::key::generate_short_id(6);
        let name = format!(
          "{}-{}.{}.c",
          cargo.spec.name, short_id, cargo.namespace_name
        );
        let spec = cargo.spec.clone();
        let container = spec.container;
        let host_config = container.host_config.unwrap_or_default();
        // Add cargo label to the container to track it
        let mut labels = container.labels.to_owned().unwrap_or_default();
        labels
          .insert("io.nanocl.c".to_owned(), cargo.spec.cargo_key.to_owned());
        labels
          .insert("io.nanocl.n".to_owned(), cargo.namespace_name.to_owned());
        labels.insert("io.nanocl.not-init-c".to_owned(), "true".to_owned());
        labels.insert(
          "com.docker.compose.project".to_owned(),
          format!("nanocl_{}", cargo.namespace_name),
        );
        let auto_remove = host_config.auto_remove.unwrap_or(false);
        if auto_remove {
          return Err(IoError::interrupted(
            "CargoCreate",
            "Auto remove is not allowed for cargo use a job instead",
          ));
        }
        let restart_policy =
          Some(host_config.restart_policy.unwrap_or(RestartPolicy {
            name: Some(RestartPolicyNameEnum::ALWAYS),
            maximum_retry_count: None,
          }));
        let env = create_cargo_env(cargo, env_secrets, current, state);
        let hostname = match &cargo.spec.container.hostname {
          None => format!("{}{}", ordinal_index, cargo.spec.name),
          Some(hostname) => format!("{}{}", ordinal_index, hostname),
        };
        // mount the secret directory to the container
        let mut binds = host_config.binds.clone().unwrap_or_default();
        binds.push(format!("{}:/opt/nanocl.io/secrets", secret_dir));
        let new_process = bollard_next::container::Config {
          attach_stderr: Some(true),
          attach_stdout: Some(true),
          tty: Some(true),
          hostname: Some(hostname),
          labels: Some(labels),
          env: Some(env),
          host_config: Some(HostConfig {
            restart_policy,
            network_mode: Some("nanoclbr0".to_owned()),
            binds: Some(binds),
            ..host_config
          }),
          ..container
        };
        super::process::create(
          &ProcessKind::Cargo,
          &name,
          &cargo.spec.cargo_key,
          &new_process,
          state,
        )
        .await
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<IoResult<Process>>>()
    .await
    .into_iter()
    .collect::<IoResult<Vec<Process>>>()?;
  Ok(instances)
}

/// Start cargo instances
///
pub async fn start(
  cargo: &Cargo,
  replicas: usize,
  state: &SystemState,
) -> IoResult<()> {
  let filter = GenericFilter::new().r#where(
    "data",
    GenericClause::Contains(serde_json::json!({
      "Config": {
        "Labels": {
          "io.nanocl.not-init-c": "true"
        }
      }
    })),
  );
  let processes = ProcessDb::read_by_kind_key(
    &cargo.spec.cargo_key,
    Some(filter),
    &state.inner.pool,
  )
  .await?;
  log::debug!(
    "processes {:?}",
    processes.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
  );
  let filter = GenericFilter::new().r#where(
    "data",
    GenericClause::Contains(serde_json::json!({
      "Config": {
        "Labels": {
          "io.nanocl.init-c": "true"
        }
      }
    })),
  );
  let init_process = ProcessDb::read_by_kind_key(
    &cargo.spec.cargo_key,
    Some(filter),
    &state.inner.pool,
  )
  .await?;
  if let Some(init_container) = &cargo.spec.init_container {
    if init_process.is_empty() {
      let process = create_init_container(cargo, init_container, state).await?;
      start_init_container(&process, state).await?;
    } else {
      start_init_container(&init_process[0], state).await?;
    }
  }
  if processes.is_empty() {
    create(cargo, replicas, state).await?;
  }
  let filter = GenericFilter::new().r#where(
    "data",
    GenericClause::Contains(serde_json::json!({
      "Config": {
        "Labels": {
          "io.nanocl.not-init-c": "true"
        }
      }
    })),
  );
  let started_instances = ProcessDb::read_by_kind_key(
    &cargo.spec.cargo_key,
    Some(filter),
    &state.inner.pool,
  )
  .await?;
  super::process::start_instances(
    &cargo.spec.cargo_key,
    &ProcessKind::Cargo,
    false,
    state,
  )
  .await?;
  let has_healthcheck = has_container_healthcheck(cargo, &started_instances);
  // Healthchecked cargo status/events are handled asynchronously in docker_event.rs.
  if has_healthcheck {
    return Ok(());
  }
  if !has_healthcheck {
    ObjPsStatusDb::update_actual_status(
      &cargo.spec.cargo_key,
      &ObjPsStatusKind::Start,
      &state.inner.pool,
    )
    .await?;
    state
      .emit_normal_native_action_sync(cargo, NativeEventAction::Start)
      .await;
  }
  Ok(())
}

/// Function that update the cargo container by creating new instances before removing the old ones
/// This way we can have zero downtime deployment
///
pub async fn update(key: &str, state: &SystemState) -> IoResult<()> {
  let cargo = CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  let processes =
    ProcessDb::read_by_kind_key(key, None, &state.inner.pool).await?;
  let old_processes = processes
    .iter()
    .filter(|process| {
      process
        .data
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref())
        .and_then(|labels| labels.get("io.nanocl.not-init-c"))
        .map(|value| value == "true")
        .unwrap_or_default()
        && !process.name.starts_with("tmp-")
    })
    .cloned()
    .collect::<Vec<_>>();
  let old_instance_keys = old_processes
    .iter()
    .map(|process| process.key.clone())
    .collect::<Vec<_>>();
  // rename old instances to flag them for deletion
  old_processes
    .iter()
    .map(|process| {
      let docker_api = state.inner.docker_api.clone();
      async move {
        if process
          .data
          .state
          .clone()
          .unwrap_or_default()
          .restarting
          .unwrap_or_default()
        {
          docker_api
            .stop_container(&process.name, None::<StopContainerOptions>)
            .await
            .map_err(|err| err.map_err_context(|| "StopContainer"))?;
        }
        let new_name = format!("tmp-{}", process.name);
        docker_api
          .rename_container(
            &process.key,
            RenameContainerOptions { name: &new_name },
          )
          .await
          .map_err(|err| err.map_err_context(|| "RenameContainer"))?;
        Ok::<_, IoError>(())
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<IoResult<Vec<_>>>()?;

  // check if any instance have port bindings
  let as_port_binding = old_processes.iter().any(|process| {
    process
      .data
      .host_config
      .as_ref()
      .and_then(|host_config| host_config.port_bindings.as_ref())
      .and_then(|port_bindings| {
        port_bindings.iter().find(|(_, bindings)| {
          bindings.iter().any(|binding| !binding.is_empty())
        })
      })
      .is_some()
  });
  // Stop old instances if they have port bindings to avoid conflicts with the new instances
  // To avoid this and have true zero downtime, use a proxy rules instead of port bindings
  if as_port_binding {
    log::debug!("cargo {key} has port bindings stopping it before update");
    super::process::stop_instances(key, &ProcessKind::Cargo, state).await?;
  }

  // Create instance with the new spec
  if let Some(init_container) = &cargo.spec.init_container {
    let process = create_init_container(&cargo, init_container, state).await?;
    start_init_container(&process, state).await?;
  }
  let new_instances = match create(&cargo, 1, state).await {
    Err(err) => {
      log::error!(
        "Unable to create cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      return Err(err);
    }
    Ok(instances) => instances,
  };
  log::debug!("cargo new instances {new_instances:?}");
  // start created containers
  let has_healthcheck;
  match super::process::start_instances(key, &ProcessKind::Cargo, false, state)
    .await
  {
    Err(err) => {
      log::error!(
        "Unable to start cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      let state_ptr_ptr = state.clone();
      let _ = super::process::delete_instances(
        &new_instances
          .iter()
          .map(|p| p.key.clone())
          .collect::<Vec<_>>(),
        &state_ptr_ptr,
      )
      .await;
      if let Err(err) =
        rename_instances_back(&old_processes, &state_ptr_ptr).await
      {
        log::error!("Unable to rename containers back: {err}");
      }
      return Err(err);
    }
    Ok(_) => {
      log::debug!("cargo instance {} started", cargo.spec.cargo_key);
      has_healthcheck = has_container_healthcheck(&cargo, &new_instances);
      if !has_healthcheck {
        // Delete old containers
        let state_ptr_ptr = state.clone();
        let old_instance_keys = old_instance_keys.clone();
        rt::spawn(async move {
          ntex::time::sleep(Duration::from_secs(4)).await;
          let _ = super::process::delete_instances(
            &old_instance_keys,
            &state_ptr_ptr,
          )
          .await;
        });
      }
    }
  }
  // Healthchecked cargo status/events are handled asynchronously in docker_event.rs.
  if has_healthcheck {
    return Ok(());
  }
  ObjPsStatusDb::update_actual_status(
    key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  state
    .emit_normal_native_action_sync(&cargo, NativeEventAction::Start)
    .await;
  Ok(())
}

/// Delete cargo instances only
///
pub async fn delete_instances(
  key: &str,
  all: bool,
  state: &SystemState,
) -> IoResult<()> {
  let node_name = state.inner.config.hostname.clone();
  let mut filter = GenericFilter::new();
  if !all {
    filter = filter.r#where("node_name", GenericClause::Eq(node_name.clone()));
  }
  let processes =
    ProcessDb::read_by_kind_key(key, Some(filter), &state.inner.pool).await?;
  log::debug!(
    "Deleting cargo instances {:?}",
    processes.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
  );
  for process in processes {
    if process.node_name == node_name {
      let _ = state
        .inner
        .docker_api
        .stop_container(&process.key, None::<StopContainerOptions>)
        .await;
      let _ = state
        .inner
        .docker_api
        .remove_container(&process.key, None::<RemoveContainerOptions>)
        .await;
    } else {
      let node =
        NodeDb::read_by_pk(&process.node_name, &state.inner.pool).await?;
      let client_opts = ConnectOpts {
        url: node.endpoint.to_owned(),
        ssl: None,
        version: Some(node.version.to_owned()),
      };
      let client = NanocldClient::connect_to(&client_opts)?;
      let params = DeleteNodeCargoParams {
        cargo_key: key.to_owned(),
      };
      client.delete_node_cargo(&params).await?;
    }
  }
  Ok(())
}

/// Delete cargo instances and the cargo itself in the database
///
pub async fn delete(key: &str, state: &SystemState) -> IoResult<()> {
  delete_instances(key, true, state).await?;
  let cargo = CargoDb::transform_read_by_pk(&key, &state.inner.pool).await?;
  CargoDb::clear_by_pk(key, &state.inner.pool).await?;
  state
    .emit_normal_native_action_sync(&cargo, NativeEventAction::Destroy)
    .await;
  Ok(())
}
