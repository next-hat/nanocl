use std::collections::HashMap;

use ntex::util::Bytes;
use futures::StreamExt;
use diesel::ExpressionMethods;
use futures_util::stream::FuturesUnordered;

use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::container::{
  Stats, ListContainersOptions, CreateContainerOptions, StartContainerOptions,
  WaitContainerOptions, RemoveContainerOptions,
};
use bollard_next::service::{
  HostConfig, ContainerSummary, RestartPolicy, RestartPolicyNameEnum,
};
use nanocl_stubs::system::EventAction;
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::generic::{GenericListNspQuery, GenericClause, GenericFilter};
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, CargoKillOptions, CargoScale, CargoStats,
  CargoStatsQuery,
};
use nanocl_stubs::cargo_spec::{
  CargoSpecPartial, CargoSpecUpdate, ReplicationMode, Config,
};

use crate::utils;
use crate::models::{
  DaemonState, CargoDb, Repository, ProcessDb, NamespaceDb, NodeDb, SecretDb,
  CargoSpecDb, FromSpec, Process, ProcessKind,
};

use super::stream::transform_stream;

/// Container to execute before the cargo instances
async fn execute_before(
  cargo: &Cargo,
  docker_api: &bollard_next::Docker,
) -> HttpResult<()> {
  match cargo.spec.init_container.clone() {
    Some(mut before) => {
      let image = before
        .image
        .clone()
        .unwrap_or(cargo.spec.container.image.clone().unwrap());
      before.image = Some(image);
      before.host_config = Some(HostConfig {
        network_mode: Some(cargo.namespace_name.clone()),
        auto_remove: Some(true),
        ..before.host_config.unwrap_or_default()
      });
      let container = docker_api
        .create_container(None::<CreateContainerOptions<String>>, before)
        .await?;
      docker_api
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await?;
      let options = Some(WaitContainerOptions {
        condition: "removed",
      });
      let mut stream = docker_api.wait_container(&container.id, options);
      while let Some(wait_status) = stream.next().await {
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
/// The number of containers created is based on the number of instances
/// defined in the cargo spec
/// If the number of instances is greater than 1, the containers will be named
/// with the cargo key and a number
/// Example: cargo-key-1, cargo-key-2, cargo-key-3
/// If the number of instances is equal to 1, the container will be named with
/// the cargo key.
async fn create_instances(
  cargo: &Cargo,
  number: usize,
  state: &DaemonState,
) -> HttpResult<Vec<Process>> {
  execute_before(cargo, &state.docker_api).await?;
  let mut secret_envs: Vec<String> = Vec::new();
  if let Some(secrets) = &cargo.spec.secrets {
    let fetched_secrets = secrets
      .iter()
      .map(|secret| async move {
        let secret = SecretDb::find_by_pk(secret, &state.pool).await??;
        if secret.kind.as_str() != "Env" {
          return Err(HttpError::bad_request(format!(
            "Secret {} is not an Env secret",
            secret.key
          )));
        }
        let envs = serde_json::from_value::<Vec<String>>(secret.data).map_err(
          |err| {
            HttpError::internal_server_error(format!(
              "Invalid secret data for secret {} {err}",
              secret.key
            ))
          },
        )?;
        Ok::<_, HttpError>(envs)
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, _>>()?;
    // Flatten the secrets
    secret_envs = fetched_secrets.into_iter().flatten().collect();
  }
  (0..number)
    .collect::<Vec<usize>>()
    .into_iter()
    .map(move |current| {
      let secret_envs = secret_envs.clone();
      async move {
        let short_id = utils::key::generate_short_id(6);
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
          return Err(HttpError::bad_request("Using autoremove for a cargo is not allowed, consider using a job instead"));
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
        let hostname = match cargo.spec.container.hostname {
          Some(ref hostname) => {
            format!("{hostname}-{short_id}")
          }
          None => name.to_owned(),
        };
        env.push(format!("NANOCL_NODE={}", state.config.hostname));
        env.push(format!("NANOCL_NODE_ADDR={}", state.config.gateway));
        env.push(format!("NANOCL_CARGO_KEY={}", cargo.spec.cargo_key.to_owned()));
        env.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
        env.push(format!("NANOCL_CARGO_INSTANCE={}", current));
        // Merge the cargo spec with the container spec
        // And set his network mode to the cargo namespace
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
        let res = utils::process::create(&name, "cargo", &cargo.spec.cargo_key, new_process, state).await?;
        Ok::<_, HttpError>(res)
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<Process, HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<Process>, HttpError>>()
}

/// Restore the instances backup. The instances are restored in parallel.
/// It's happenning if when a cargo fail to updates.
async fn restore_instances_backup(
  instances: &[ContainerSummary],
  state: &DaemonState,
) -> HttpResult<()> {
  instances
    .iter()
    .map(|container| async {
      let id = container.id.clone().unwrap_or_default();
      let container_state = container.state.clone().unwrap_or_default();
      if container_state == "restarting" {
        state.docker_api.stop_container(&id, None).await?;
      }
      let names = container.names.clone().unwrap_or_default();
      let name = format!("{}-backup", names[0]);
      state
        .docker_api
        .rename_container(
          &id,
          bollard_next::container::RenameContainerOptions { name },
        )
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<(), _>>()
}

/// Rename the containers of the given cargo by adding `-backup` to the name
/// of the container to mark them as backup.
/// In case of failure, the backup containers are restored.
async fn rename_instances_original(
  instances: &[ContainerSummary],
  state: &DaemonState,
) -> HttpResult<()> {
  instances
    .iter()
    .map(|container| async {
      let id = container.id.clone().unwrap_or_default();
      let container_state = container.state.clone().unwrap_or_default();
      if container_state == "restarting" {
        state.docker_api.stop_container(&id, None).await?;
      }
      let names = container.names.clone().unwrap_or_default();
      let name = names[0].replace("-backup", "");
      state
        .docker_api
        .rename_container(
          &id,
          bollard_next::container::RenameContainerOptions { name },
        )
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<(), _>>()
}

/// The instances (containers) are deleted but the cargo is not.
/// The cargo is not deleted because it can be used to restore the containers.
async fn delete_instances(
  instances: &[String],
  state: &DaemonState,
) -> HttpResult<()> {
  instances
    .iter()
    .map(|id| async {
      utils::process::remove(
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
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<(), _>>()
}

/// List the cargo instances (containers) based on the cargo key
pub(crate) async fn list_instances(
  key: &str,
  docker_api: &bollard_next::Docker,
) -> HttpResult<Vec<ContainerSummary>> {
  let label = format!("io.nanocl.c={key}");
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

/// Create a cargo based on the given partial spec
/// And create his instances (containers).
pub(crate) async fn create(
  namespace: &str,
  spec: &CargoSpecPartial,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo =
    CargoDb::create_from_spec(namespace, spec, version, &state.pool).await?;
  let number = if let Some(mode) = &cargo.spec.replication {
    match mode {
      ReplicationMode::Static(replication_static) => replication_static.number,
      ReplicationMode::Auto => 1,
      ReplicationMode::Unique => 1,
      ReplicationMode::UniqueByNode => 1,
      _ => 1,
    }
  } else {
    1
  };
  if let Err(err) = create_instances(&cargo, number, state).await {
    CargoDb::delete_by_pk(&cargo.spec.cargo_key, &state.pool).await??;
    return Err(err);
  }
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Created);
  Ok(cargo)
}

/// Restart cargo instances (containers) by key
pub(crate) async fn restart(
  key: &str,
  docker_api: &bollard_next::Docker,
) -> HttpResult<()> {
  let containers = list_instances(key, docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      let id = container.id.unwrap_or_default();
      let docker_api = docker_api.clone();
      docker_api
        .restart_container(&id, None)
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  Ok(())
}

/// Delete a cargo by key with his given instances (containers).
pub(crate) async fn delete_by_key(
  key: &str,
  force: Option<bool>,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo = CargoDb::inspect_by_pk(key, &state.pool).await?;
  let containers = list_instances(key, &state.docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      utils::process::remove(
        &container.id.unwrap_or_default(),
        Some(RemoveContainerOptions {
          force: force.unwrap_or(false),
          ..Default::default()
        }),
        state,
      )
      .await
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  CargoDb::delete_by_pk(key, &state.pool).await??;
  CargoSpecDb::delete_by(
    crate::schema::cargo_specs::dsl::cargo_key.eq(key.to_owned()),
    &state.pool,
  )
  .await??;
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Deleted);
  Ok(())
}

/// A new history entry is added and the containers are updated
/// with the new cargo specification
pub(crate) async fn put(
  cargo_key: &str,
  cargo_partial: &CargoSpecPartial,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo =
    CargoDb::update_from_spec(cargo_key, cargo_partial, version, &state.pool)
      .await?;
  // Get the number of instance to create
  let number = if let Some(mode) = &cargo.spec.replication {
    match mode {
      ReplicationMode::Static(replication_static) => replication_static.number,
      ReplicationMode::Auto => 1,
      ReplicationMode::Unique => 1,
      ReplicationMode::UniqueByNode => 1,
      _ => 1,
    }
  } else {
    1
  };
  let containers = list_instances(cargo_key, &state.docker_api).await?;
  restore_instances_backup(&containers, state).await?;
  // Create instance with the new spec
  let new_instances = match create_instances(&cargo, number, state).await {
    // If the creation of the new instance failed, we rename the old containers
    Err(err) => {
      log::warn!("Unable to create cargo instance: {}", err);
      log::warn!("Rollback to previous instance");
      rename_instances_original(&containers, state).await?;
      Vec::default()
    }
    Ok(instances) => instances,
  };
  // start created containers
  match utils::process::start_by_kind(&ProcessKind::Cargo, cargo_key, state)
    .await
  {
    Err(err) => {
      log::error!(
        "Unable to start cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      delete_instances(
        &new_instances
          .iter()
          .map(|i| i.key.clone())
          .collect::<Vec<_>>(),
        state,
      )
      .await?;
      rename_instances_original(&containers, state).await?;
    }
    Ok(_) => {
      // Delete old containers
      delete_instances(
        &containers
          .iter()
          .map(|c| c.id.clone().unwrap_or_default())
          .collect::<Vec<_>>(),
        state,
      )
      .await?;
    }
  }
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Patched);
  Ok(cargo)
}

/// List the cargoes for the given query
pub(crate) async fn list(
  query: &GenericListNspQuery,
  state: &DaemonState,
) -> HttpResult<Vec<CargoSummary>> {
  let namespace = utils::key::resolve_nsp(&query.namespace);
  let filter = GenericFilter::try_from(query.clone())
    .map_err(|err| {
      HttpError::bad_request(format!("Invalid query string: {}", err))
    })?
    .r#where("namespace_name", GenericClause::Eq(namespace.clone()));
  // ensure namespace exists
  NamespaceDb::find_by_pk(&namespace, &state.pool).await??;
  let cargoes = CargoDb::find(&filter, &state.pool).await??;
  let mut cargo_summaries = Vec::new();
  for cargo in cargoes {
    let spec = CargoSpecDb::find_by_pk(&cargo.spec.key, &state.pool)
      .await??
      .try_to_spec()?;
    let instances =
      ProcessDb::find_by_kind_key(&cargo.spec.cargo_key, &state.pool).await?;
    let mut running_instances = 0;
    for instance in &instances {
      let state = instance.data.state.clone().unwrap_or_default();
      if state.restarting.unwrap_or_default() {
        continue;
      }
      if state.running.unwrap_or_default() {
        running_instances += 1;
      }
    }
    cargo_summaries.push(CargoSummary {
      created_at: cargo.created_at,
      namespace_name: cargo.namespace_name,
      instance_total: instances.len(),
      instance_running: running_instances,
      spec: spec.clone(),
    });
  }
  Ok(cargo_summaries)
}

/// Return detailed information about the cargo for the given key
pub(crate) async fn inspect_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<CargoInspect> {
  let cargo = CargoDb::inspect_by_pk(key, &state.pool).await?;
  let mut running_instances = 0;
  let instances = ProcessDb::find_by_kind_key(key, &state.pool).await?;
  let nodes = NodeDb::find(&GenericFilter::default(), &state.pool).await??;
  // Convert into a hashmap for faster lookup
  let nodes = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<String, _>>();
  let instances = instances
    .into_iter()
    .map(|instance| {
      let node_instance = NodeContainerSummary {
        node: instance.node_key.clone(),
        ip_address: match nodes.get(&instance.node_key) {
          Some(node) => node.ip_address.clone(),
          None => "Unknow".to_owned(),
        },
        container: instance.data.clone(),
      };
      if instance
        .data
        .state
        .unwrap_or_default()
        .running
        .unwrap_or_default()
      {
        running_instances += 1;
      }
      node_instance
    })
    .collect::<Vec<_>>();
  Ok(CargoInspect {
    created_at: cargo.created_at,
    namespace_name: cargo.namespace_name,
    instance_total: instances.len(),
    instance_running: running_instances,
    spec: cargo.spec,
    instances,
  })
}

/// This remove all cargo in the given namespace and all their instances (containers)
/// from the system (database and docker).
pub(crate) async fn delete_by_namespace(
  namespace: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let namespace = NamespaceDb::find_by_pk(namespace, &state.pool).await??;
  let cargoes =
    CargoDb::find_by_namespace(&namespace.name, &state.pool).await?;
  cargoes
    .into_iter()
    .map(|cargo| async move {
      delete_by_key(&cargo.spec.cargo_key, None, state).await
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, HttpError>>()?;
  Ok(())
}

/// Send a signal to a cargo instance the cargo name can be used if the cargo has only one instance
/// The signal is send to one instance only
pub(crate) async fn kill_by_key(
  key: &str,
  options: &CargoKillOptions,
  state: &DaemonState,
) -> HttpResult<()> {
  let instances = ProcessDb::find_by_kind_key(key, &state.pool).await?;
  if instances.is_empty() {
    return Err(HttpError::not_found(format!(
      "Cargo instance not found: {key}"
    )));
  }
  let id = instances[0].data.id.clone().unwrap_or_default();
  let options = options.clone().into();
  state.docker_api.kill_container(&id, Some(options)).await?;
  Ok(())
}

/// Merge the given cargo spec with the existing one
pub async fn patch(
  key: &str,
  payload: &CargoSpecUpdate,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo = CargoDb::inspect_by_pk(key, &state.pool).await?;
  let container = if let Some(container) = payload.container.clone() {
    // merge env and ensure no duplicate key
    let new_env = container.env.unwrap_or_default();
    let mut env_vars: Vec<String> =
      cargo.spec.container.env.unwrap_or_default();
    // Merge environment variables from new_env into the merged array
    for env_var in new_env {
      let parts: Vec<&str> = env_var.split('=').collect();
      if parts.len() != 2 {
        continue;
      }
      let name = parts[0].to_owned();
      let value = parts[1].to_owned();
      if let Some(pos) = env_vars.iter().position(|x| x.starts_with(&name)) {
        let old_value = env_vars[pos].split('=').nth(1).unwrap().to_owned();
        if old_value != value && !value.is_empty() {
          // Update the value if it has changed
          env_vars[pos] = format!("{}={}", name, value);
        } else if value.is_empty() {
          // Remove the variable if the value is empty
          env_vars.remove(pos);
        }
      } else {
        // Add new environment variables
        env_vars.push(env_var);
      }
    }
    // merge volumes and ensure no duplication
    let new_volumes = container
      .host_config
      .clone()
      .unwrap_or_default()
      .binds
      .unwrap_or_default();
    let mut volumes: Vec<String> = cargo
      .spec
      .container
      .host_config
      .clone()
      .unwrap_or_default()
      .binds
      .unwrap_or_default();
    for volume in new_volumes {
      if !volumes.contains(&volume) {
        volumes.push(volume);
      }
    }
    let image = if let Some(image) = container.image.clone() {
      Some(image)
    } else {
      cargo.spec.container.image
    };
    let cmd = if let Some(cmd) = container.cmd {
      Some(cmd)
    } else {
      cargo.spec.container.cmd
    };
    Config {
      cmd,
      image,
      env: Some(env_vars),
      host_config: Some(HostConfig {
        binds: Some(volumes),
        ..cargo.spec.container.host_config.unwrap_or_default()
      }),
      ..cargo.spec.container
    }
  } else {
    cargo.spec.container
  };
  let spec = CargoSpecPartial {
    name: cargo.spec.name.clone(),
    container,
    init_container: if payload.init_container.is_some() {
      payload.init_container.clone()
    } else {
      cargo.spec.init_container
    },
    replication: payload.replication.clone(),
    secrets: if payload.secrets.is_some() {
      payload.secrets.clone()
    } else {
      cargo.spec.secrets
    },
    metadata: if payload.metadata.is_some() {
      payload.metadata.clone()
    } else {
      cargo.spec.metadata
    },
  };
  utils::cargo::put(key, &spec, version, state).await
}

/// Get the stats of a cargo instance
/// The cargo name can be used if the cargo has only one instance
pub(crate) fn get_stats(
  name: &str,
  query: &CargoStatsQuery,
  docker_api: &bollard_next::Docker,
) -> HttpResult<impl StreamExt<Item = HttpResult<Bytes>>> {
  let stream =
    docker_api.stats(&format!("{name}.c"), Some(query.clone().into()));
  let stream = transform_stream::<Stats, CargoStats>(stream);
  Ok(stream)
}

/// Scale a cargo instance up or down to the given number of instances (containers, replicas)
pub async fn scale(
  key: &str,
  options: &CargoScale,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo = CargoDb::inspect_by_pk(key, &state.pool).await?;
  let instances = ProcessDb::find_by_kind_key(key, &state.pool).await?;
  let is_equal = usize::try_from(options.replicas)
    .map(|replica| instances.len() == replica)
    .unwrap_or(false);
  if is_equal {
    return Ok(());
  }
  if options.replicas.is_negative() {
    let to_remove = options.replicas.unsigned_abs();
    instances
      .iter()
      .take(to_remove)
      .map(|instance| {
        utils::process::remove(
          &instance.key,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
          state,
        )
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<_, HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, HttpError>>()?;
  } else {
    let cargo = CargoDb::inspect_by_pk(key, &state.pool).await?;
    let to_add = options.replicas.unsigned_abs();
    let created_instances = create_instances(&cargo, to_add, state).await?;
    created_instances
      .iter()
      .map(|instance| async {
        state
          .docker_api
          .start_container::<String>(&instance.key, None)
          .await?;
        Ok::<_, HttpError>(())
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<_, HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, HttpError>>()?;
  }
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Patched);
  Ok(())
}
