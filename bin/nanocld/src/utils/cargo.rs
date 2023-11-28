use std::collections::HashMap;

use ntex::util::Bytes;
use futures::StreamExt;
use futures_util::TryFutureExt;
use futures_util::stream::FuturesUnordered;
use bollard_next::service::ContainerCreateResponse;

use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::container::{
  Stats, LogOutput, ListContainersOptions, CreateContainerOptions,
  StartContainerOptions, WaitContainerOptions, RemoveContainerOptions,
};
use bollard_next::service::{
  HostConfig, ContainerSummary, RestartPolicy, RestartPolicyNameEnum,
};
use nanocl_stubs::system::EventAction;
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, OutputLog, CargoLogQuery,
  CargoKillOptions, GenericCargoListQuery, CargoScale, CargoStats,
  CargoStatsQuery,
};
use nanocl_stubs::cargo_spec::{
  CargoSpecPartial, CargoSpecUpdate, ReplicationMode, Config,
};

use crate::{utils, repositories};
use crate::models::DaemonState;

use super::stream::transform_stream;

/// ## Execute before
///
/// Container to execute before the cargo instances
///
/// ## Arguments
///
/// * [cargo](Cargo) - The cargo
/// * [docker_api](bollard_next::Docker) - The docker api
///
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

/// ## Create instances
///
/// Create instances (containers) based on the cargo spec
/// The number of containers created is based on the number of instances
/// defined in the cargo spec
/// If the number of instances is greater than 1, the containers will be named
/// with the cargo key and a number
/// Example: cargo-key-1, cargo-key-2, cargo-key-3
/// If the number of instances is equal to 1, the container will be named with
/// the cargo key.
///
/// ## Arguments
///
/// * [cargo](Cargo) - The cargo
/// * [start](usize) - The number of already created containers
/// * [number](usize) - The number of containers to create
/// * [docker_api](bollard_next::Docker) - The docker api
///
/// ## Return
///
/// [HttpResult][HttpResult] containing a [Vec](Vec) of [ContainerCreateResponse][ContainerCreateResponse]
///
async fn create_instances(
  cargo: &Cargo,
  start: usize,
  number: usize,
  state: &DaemonState,
) -> HttpResult<Vec<ContainerCreateResponse>> {
  execute_before(cargo, &state.docker_api).await?;
  let mut secret_envs: Vec<String> = Vec::new();
  if let Some(secrets) = &cargo.spec.secrets {
    let fetched_secrets = secrets
      .iter()
      .map(|secret| async move {
        let secret =
          repositories::secret::find_by_key(secret, &state.pool).await?;
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
        let name = if current > 0 || start > 0 {
          format!("{}-{}.c", current + start, cargo.spec.cargo_key)
        } else {
          format!("{}.c", cargo.spec.cargo_key)
        };
        let create_options = bollard_next::container::CreateContainerOptions {
          name: name.clone(),
          ..Default::default()
        };
        let spec = cargo.spec.clone();
        let container = spec.container;
        let host_config = container.host_config.unwrap_or_default();
        // Add cargo label to the container to track it
        let mut labels = container.labels.to_owned().unwrap_or_default();
        labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
        labels.insert("io.nanocl.kind".to_owned(), "Cargo".to_owned());
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
            if current > 0 {
              format!("{current}-{hostname}")
            } else {
              hostname.to_owned()
            }
          }
          None => name.replace('.', "-"),
        };
        env.push(format!("NANOCL_NODE={}", state.config.hostname));
        env.push(format!("NANOCL_NODE_ADDR={}", state.config.gateway));
        env.push(format!("NANOCL_CARGO_KEY={}", cargo.spec.cargo_key));
        env.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
        env.push(format!("NANOCL_CARGO_INSTANCE={}", current));
        // Merge the cargo spec with the container spec
        // And set his network mode to the cargo namespace
        let hook_container = bollard_next::container::Config {
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
        let res = state
          .docker_api
          .create_container::<String>(Some(create_options), hook_container)
          .map_err(HttpError::from)
          .await?;
        Ok::<_, HttpError>(res)
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<ContainerCreateResponse, HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<ContainerCreateResponse>, HttpError>>()
}

/// ## Restore instances backup
///
/// Restore the instances backup. The instances are restored in parallel.
/// It's happenning if when a cargo fail to updates.
///
/// ## Arguments
///
/// * [instances](Vec<ContainerSummary>) - The instances to restore
/// * [state](DaemonState) - The daemon state
///
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

/// ## Rename instances original
///
/// Rename the containers of the given cargo by adding `-backup` to the name
/// of the container to mark them as backup.
/// In case of failure, the backup containers are restored.
///
/// ## Arguments
///
/// * [instances](Vec<ContainerSummary>) - The instances to rename
/// * [state](DaemonState) - The daemon state
///
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

/// ## Delete instances
///
/// The instances (containers) are deleted but the cargo is not.
/// The cargo is not deleted because it can be used to restore the containers.
///
/// ## Arguments
///
/// * [instances](Vec<ContainerSummary>) - The instances to delete
/// * [state](DaemonState) - The daemon state
///
async fn delete_instances(
  instances: &[String],
  state: &DaemonState,
) -> HttpResult<()> {
  instances
    .iter()
    .map(|id| async {
      state
        .docker_api
        .remove_container(
          &id.clone(),
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
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

/// ## List instances
///
/// List the cargo instances (containers) based on the cargo key
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [docker_api](bollard_next::Docker) - The docker api
///
/// ## Return
///
/// [HttpResult][HttpResult] containing a [Vec](Vec) of [ContainerSummary][ContainerSummary]
///
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

/// ## Create
///
/// Create a cargo based on the given partial spec
/// And create his instances (containers).
///
/// ## Arguments
///
/// * [namespace](str) - The namespace
/// * [spec](CargoSpecPartial) - The cargo spec partial
/// * [version](str) - The cargo version
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [cargo](Cargo)
///
pub(crate) async fn create(
  namespace: &str,
  spec: &CargoSpecPartial,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo =
    repositories::cargo::create(namespace, spec, version, &state.pool).await?;
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
  if let Err(err) = create_instances(&cargo, 0, number, state).await {
    repositories::cargo::delete_by_key(&cargo.spec.cargo_key, &state.pool)
      .await?;
    return Err(err);
  }
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Created);
  Ok(cargo)
}

/// ## Start by key
///
/// The cargo instances (containers) are started in parallel
/// If one container fails to start, the other containers will continue to start
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn start_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo_key = key.to_owned();
  let docker_api = state.docker_api.clone();
  let cargo =
    repositories::cargo::inspect_by_key(&cargo_key, &state.pool).await?;
  let containers = list_instances(&cargo_key, &docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      let id = container.id.unwrap_or_default();
      docker_api.start_container::<String>(&id, None).await?;
      Ok::<_, HttpError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Started);
  Ok(())
}

/// ## Stop by key
///
/// Stop all instances (containers) for the given cargo key.
/// The containers are stopped in parallel.
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn stop_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let containers = list_instances(key, &state.docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      let id = container.id.unwrap_or_default();
      state
        .docker_api
        .stop_container(&id, None)
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Stopped);
  Ok(())
}

/// ## Restart by key
///
/// Restart cargo instances (containers) by key
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [docker_api](bollard_next::Docker) - The docker api
///
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

/// ## Delete by key
///
/// Delete a cargo by key with his given instances (containers).
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [force](Option<bool>) - Force the deletion of the cargo
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn delete_by_key(
  key: &str,
  force: Option<bool>,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let containers = list_instances(key, &state.docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      state
        .docker_api
        .remove_container(
          &container.id.unwrap_or_default(),
          Some(RemoveContainerOptions {
            force: force.unwrap_or(false),
            ..Default::default()
          }),
        )
        .await
        .map_err(HttpError::from)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  repositories::cargo::delete_by_key(key, &state.pool).await?;
  repositories::cargo_spec::delete_by_cargo_key(key, &state.pool).await?;
  state
    .event_emitter
    .spawn_emit_to_event(&cargo, EventAction::Deleted);
  Ok(())
}

/// ## Put
///
/// A new history entry is added and the containers are updated
/// with the new cargo specification
///
/// ## Arguments
/// * [cargo_key](str) - The cargo key
/// * [cargo_partial](CargoSpecPartial) - The cargo spec
/// * [version](str) - The version of the api to use
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [cargo](Cargo)
///
pub(crate) async fn put(
  cargo_key: &str,
  cargo_partial: &CargoSpecPartial,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo = repositories::cargo::update_by_key(
    cargo_key,
    cargo_partial,
    version,
    &state.pool,
  )
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
  let new_instances = match create_instances(&cargo, 0, number, state).await {
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
  match start_by_key(cargo_key, state).await {
    Err(err) => {
      log::error!(
        "Unable to start cargo instance {} : {err}",
        cargo.spec.cargo_key
      );
      delete_instances(
        &new_instances
          .iter()
          .map(|i| i.id.clone())
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

/// ## List
///
/// List the cargoes for the given query
///
/// ## Arguments
///
/// * [query](GenericCargoListQuery) - The filter query
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [CargoSummary][CargoSummary
///
pub(crate) async fn list(
  query: GenericCargoListQuery<&str>,
  state: &DaemonState,
) -> HttpResult<Vec<CargoSummary>> {
  let namespace =
    repositories::namespace::find_by_name(query.namespace, &state.pool).await?;
  let query = query.merge(namespace);
  let cargoes = repositories::cargo::list_by_query(&query, &state.pool).await?;
  let mut cargo_summaries = Vec::new();
  for cargo in cargoes {
    let spec =
      repositories::cargo_spec::find_by_key(&cargo.spec_key, &state.pool)
        .await?;
    let instances = repositories::container_instance::list_for_kind(
      "Cargo",
      &cargo.key,
      &state.pool,
    )
    .await?;
    let mut running_instances = 0;
    for instance in &instances {
      let is_running = instance
        .data
        .state
        .clone()
        .unwrap_or_default()
        .running
        .unwrap_or_default();
      if is_running {
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

/// ## Inspect by key
///
/// Return detailed information about the cargo for the given key
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [cargo](CargoInspect)
///
pub(crate) async fn inspect_by_key(
  key: &str,
  state: &DaemonState,
) -> HttpResult<CargoInspect> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let mut running_instances = 0;
  let instances =
    repositories::container_instance::list_for_kind("Cargo", key, &state.pool)
      .await?;
  let nodes = repositories::node::list(&state.pool).await?;
  // Convert into a hashmap for faster lookup
  let nodes = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<String, _>>();
  let instances = instances
    .into_iter()
    .map(|instance| {
      let node_instance = NodeContainerSummary {
        node: instance.node_id.clone(),
        ip_address: match nodes.get(&instance.node_id) {
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

/// ## Delete by namespace
///
/// This remove all cargo in the given namespace and all their instances (containers)
/// from the system (database and docker).
///
/// ## Arguments
///
/// * [namespace](str) - The namespace name
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn delete_by_namespace(
  namespace: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  let namespace =
    repositories::namespace::find_by_name(namespace, &state.pool).await?;
  let cargoes =
    repositories::cargo::find_by_namespace(&namespace, &state.pool).await?;
  cargoes
    .into_iter()
    .map(|cargo| async move { delete_by_key(&cargo.key, None, state).await })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<(), HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, HttpError>>()?;
  Ok(())
}

/// ## Kill by name
///
/// Send a signal to a cargo instance the cargo name can be used if the cargo has only one instance
/// The signal is send to one instance only
///
/// ## Arguments
///
/// * [name](str) - The cargo name
/// * [options](CargoKillOptions) - The kill options
/// * [docker_api](bollard_next::Docker) - The docker api
///
pub(crate) async fn kill_by_name(
  name: &str,
  options: &CargoKillOptions,
  docker_api: &bollard_next::Docker,
) -> HttpResult<()> {
  let name = format!("{name}.c");
  let options = options.clone().into();
  docker_api.kill_container(&name, Some(options)).await?;
  Ok(())
}

/// ## Patch
///
/// Merge the given cargo spec with the existing one
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [payload](CargoSpecUpdate) - The cargo spec update
/// * [version](str) - The cargo version
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [cargo](Cargo)
///
pub async fn patch(
  key: &str,
  payload: &CargoSpecUpdate,
  version: &str,
  state: &DaemonState,
) -> HttpResult<Cargo> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
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

/// ## Get logs
///
/// Get the logs of a cargo instance
/// The cargo name can be used if the cargo has only one instance
/// The query parameter can be used to filter the logs
///
/// ## Arguments
///
/// * [name](str): The cargo name
/// * [query](CargoLogQuery): The query parameters
/// * [docker_api](bollard_next::Docker): The docker api
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [stream](StreamExt) of [LogOutput](LogOutput)
///
pub(crate) fn get_logs(
  name: &str,
  query: &CargoLogQuery,
  docker_api: &bollard_next::Docker,
) -> HttpResult<impl StreamExt<Item = HttpResult<Bytes>>> {
  let stream =
    docker_api.logs(&format!("{name}.c"), Some(query.clone().into()));
  let stream = transform_stream::<LogOutput, OutputLog>(stream);
  Ok(stream)
}

/// ## Get stats
///
/// Get the stats of a cargo instance
/// The cargo name can be used if the cargo has only one instance
///
/// ## Arguments
///
/// * [name](str): The cargo name
/// * [query](CargoStatsQuery): The query parameters
/// * [docker_api](bollard_next::Docker): The docker api
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [stream](StreamExt) of [CargoStats](CargoStats)
///
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

/// ## Scale
///
/// Scale a cargo instance up or down to the given number of instances (containers, replicas)
///
/// ## Arguments
///
/// * [key](str) - The cargo key
/// * [options](CargoScale) - The scale options
/// * [state](DaemonState) - The daemon state
///
pub async fn scale(
  key: &str,
  options: &CargoScale,
  state: &DaemonState,
) -> HttpResult<()> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let instances = list_instances(key, &state.docker_api).await?;
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
      .map(|instance| async {
        state
          .docker_api
          .remove_container(
            &instance.id.clone().unwrap_or_default(),
            Some(RemoveContainerOptions {
              force: true,
              ..Default::default()
            }),
          )
          .await?;
        Ok::<_, HttpError>(())
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<_, HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, HttpError>>()?;
  } else {
    let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
    let to_add = options.replicas.unsigned_abs();
    let created_instances =
      create_instances(&cargo, instances.len(), to_add, state).await?;
    created_instances
      .iter()
      .map(|instance| async {
        state
          .docker_api
          .start_container::<String>(&instance.id, None)
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
