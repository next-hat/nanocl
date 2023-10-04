use std::collections::HashMap;

use nanocl_stubs::cargo::CargoStatsQuery;
use ntex::rt;
use ntex::util::Bytes;
use futures::{StreamExt, TryStreamExt};
use futures_util::TryFutureExt;
use futures_util::stream::FuturesUnordered;
use bollard_next::service::ContainerCreateResponse;

use bollard_next::container::LogOutput;
use bollard_next::container::WaitContainerOptions;
use bollard_next::service::{ContainerSummary, HostConfig};
use bollard_next::service::{RestartPolicy, RestartPolicyNameEnum};
use bollard_next::container::{
  ListContainersOptions, RemoveContainerOptions, Stats,
};

use nanocl_utils::http_error::HttpError;
use nanocl_stubs::node::NodeContainerSummary;
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, OutputLog, CargoLogQuery,
  CargoKillOptions, GenericCargoListQuery, CargoScale, CargoStats,
};
use nanocl_stubs::cargo_config::{
  CargoConfigPartial, CargoConfigUpdate, ReplicationMode,
  Config as ContainerConfig,
};

use crate::models::DaemonState;
use crate::{utils, repositories};

use super::stream::transform_stream;

/// ## Create instances
///
/// Create instances (containers) based on the cargo config
/// The number of containers created is based on the number of instances
/// defined in the cargo config
/// If the number of instances is greater than 1, the containers will be named
/// with the cargo key and a number
/// Example: cargo-key-1, cargo-key-2, cargo-key-3
/// If the number of instances is equal to 1, the container will be named with
/// the cargo key.
///
/// ## Arguments
///
/// - [cargo](Cargo) - The cargo
/// - [number](i64) - The number of containers to create
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been created
///   - [Err](HttpError) - The containers has not been created
///
async fn create_instances(
  cargo: &Cargo,
  start: usize,
  number: usize,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<ContainerCreateResponse>, HttpError> {
  (0..number)
    .collect::<Vec<usize>>()
    .into_iter()
    .map(move |current| async move {
      let name = if current > 0 || start > 0 {
        format!("{}-{}.c", current + start, cargo.key)
      } else {
        format!("{}.c", cargo.key)
      };
      let create_options = bollard_next::container::CreateContainerOptions {
        name: name.clone(),
        ..Default::default()
      };
      // Add cargo label to the container to track it
      let mut labels =
        cargo.config.container.labels.to_owned().unwrap_or_default();
      labels.insert("io.nanocl".into(), "enabled".into());
      labels.insert("io.nanocl.c".into(), cargo.key.to_owned());
      labels.insert("io.nanocl.n".into(), cargo.namespace_name.to_owned());
      labels.insert("io.nanocl.cnsp".into(), cargo.namespace_name.to_owned());
      labels.insert(
        "com.docker.compose.project".into(),
        format!("nanocl_{}", cargo.namespace_name),
      );
      let auto_remove = cargo
        .config
        .to_owned()
        .container
        .host_config
        .unwrap_or_default()
        .auto_remove
        .unwrap_or(false);
      let restart_policy = if auto_remove {
        None
      } else {
        Some(
          cargo
            .config
            .to_owned()
            .container
            .host_config
            .unwrap_or_default()
            .restart_policy
            .unwrap_or(RestartPolicy {
              name: Some(RestartPolicyNameEnum::ALWAYS),
              maximum_retry_count: None,
            }),
        )
      };
      let mut env = cargo.config.container.env.clone().unwrap_or_default();
      let hostname = match cargo.config.container.hostname {
        Some(ref hostname) => {
          if current > 0 {
            format!("{current}-{hostname}")
          } else {
            hostname.to_owned()
          }
        }
        None => name.replace('.', "-"),
      };
      env.push(format!("NANOCL_CARGO_KEY={}", cargo.key));
      env.push(format!("NANOCL_CARGO_NAMESPACE={}", cargo.namespace_name));
      env.push(format!("NANOCL_CARGO_INSTANCE={}", current));
      // Merge the cargo config with the container config
      // And set his network mode to the cargo namespace
      let config = bollard_next::container::Config {
        attach_stderr: Some(true),
        attach_stdout: Some(true),
        tty: Some(true),
        hostname: Some(hostname),
        labels: Some(labels),
        env: Some(env),
        host_config: Some(HostConfig {
          restart_policy,
          network_mode: Some(
            cargo
              .config
              .to_owned()
              .container
              .host_config
              .unwrap_or_default()
              .network_mode
              .unwrap_or(cargo.namespace_name.to_owned()),
          ),
          ..cargo
            .config
            .to_owned()
            .container
            .host_config
            .unwrap_or_default()
        }),
        ..cargo.config.container.to_owned()
      };
      let res = docker_api
        .create_container::<String>(Some(create_options), config)
        .map_err(HttpError::from)
        .await?;
      Ok::<_, HttpError>(res)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<ContainerCreateResponse, HttpError>>>()
    .await
    .into_iter()
    .collect::<Result<Vec<ContainerCreateResponse>, HttpError>>()
}

/// ## List instances
///
/// List the cargo instances (containers) based on the cargo key
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers have been listed
///   - [Err](HttpError) - The containers have not been listed
///
pub async fn list_instances(
  key: &str,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<ContainerSummary>, HttpError> {
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
/// Create a cargo based on the given partial config
/// And create his instances (containers).
///
/// ## Arguments
///
/// - [namespace](str) - The namespace
/// - [config](CargoConfigPartial) - The cargo config partial
/// - [version](str) - The cargo version
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been created
///   - [Err](HttpError) - The cargo has not been created
///
pub async fn create(
  namespace: &str,
  config: &CargoConfigPartial,
  version: &str,
  state: &DaemonState,
) -> Result<Cargo, HttpError> {
  let cargo =
    repositories::cargo::create(namespace, config, version, &state.pool)
      .await?;
  let number = if let Some(mode) = &cargo.config.replication {
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
  if let Err(err) = create_instances(&cargo, 0, number, &state.docker_api).await
  {
    repositories::cargo::delete_by_key(&cargo.key, &state.pool).await?;
    return Err(err);
  }
  Ok(cargo)
}

/// ## Start by key
///
/// The cargo instances (containers) are started in parallel
/// If one container fails to start, the other containers will continue to start
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been started
///   - [Err](HttpError) - The containers has not been started
///
pub async fn start_by_key(
  key: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let cargo_key = key.to_owned();
  let docker_api = state.docker_api.clone();
  let cargo =
    repositories::cargo::inspect_by_key(&cargo_key, &state.pool).await?;
  let auto_remove = cargo
    .config
    .container
    .host_config
    .unwrap_or_default()
    .auto_remove
    .unwrap_or(false);
  let containers = list_instances(&cargo_key, &docker_api).await?;
  let mut autoremove_futs = Vec::new();
  let mut futs = Vec::new();
  for container in containers {
    let id = container.id.unwrap_or_default();
    if auto_remove {
      let id = id.clone();
      let docker_api = docker_api.clone();
      autoremove_futs.push(async move {
        let id = id.clone();
        let options = Some(WaitContainerOptions {
          condition: "removed",
        });
        let stream = docker_api.wait_container(&id, options);
        if let Err(err) = stream.try_for_each(|_| async { Ok(()) }).await {
          log::warn!("Error while waiting for container {id} {err}");
        }
      });
    }
    let id = id.clone();
    let docker_api = docker_api.clone();
    let fut = async move {
      if let Err(err) = docker_api.start_container::<String>(&id, None).await {
        log::warn!("Error while starting container {id} {err}");
      }
    };
    futs.push(fut);
  }
  let _ = FuturesUnordered::from_iter(futs).collect::<Vec<_>>().await;
  if auto_remove {
    let pool = state.pool.clone();
    rt::spawn(async move {
      let _ = FuturesUnordered::from_iter(autoremove_futs)
        .collect::<Vec<_>>()
        .await;
      if let Err(err) =
        repositories::cargo::delete_by_key(&cargo_key, &pool).await
      {
        log::warn!("Error while deleting cargo {cargo_key} {err}");
      }
    });
  }
  Ok(())
}

/// ## Stop by key
///
/// Stop all instances (containers) for the given cargo key.
/// The containers are stopped in parallel.
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been stopped
///   - [Err](HttpError) - The containers has not been stopped
///
pub async fn stop_by_key(
  key: &str,
  docker_api: &bollard_next::Docker,
) -> Result<(), HttpError> {
  let containers = list_instances(key, docker_api).await?;
  containers
    .into_iter()
    .map(|container| async {
      let id = container.id.unwrap_or_default();
      let docker_api = docker_api.clone();
      docker_api
        .stop_container(&id, None)
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

/// ## Restart by key
///
/// Restart cargo instances (containers) by key
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been restarted
///   - [Err](HttpError) - The containers has not been restarted
///
pub async fn restart(
  key: &str,
  docker_api: &bollard_next::Docker,
) -> Result<(), HttpError> {
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

/// ## Restore instances backup
///
/// Restore the instances backup. The instances are restored in parallel.
/// It's happenning if when a cargo fail to updates.
///
/// ## Arguments
///
/// - [instances](Vec<ContainerSummary>) - The instances to restore
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The instances has been restored
///   - [Err](HttpError) - The instances has not been restored
///
async fn restore_instances_backup(
  instances: &[ContainerSummary],
  state: &DaemonState,
) -> Result<(), HttpError> {
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
/// - [instances](Vec<ContainerSummary>) - The instances to rename
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been renamed
///   - [Err](HttpError) - The containers has not been renamed
///
async fn rename_instances_original(
  instances: &[ContainerSummary],
  state: &DaemonState,
) -> Result<(), HttpError> {
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

/// ## Delete by key
///
/// Delete a cargo by key with his given instances (containers).
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [force](Option<bool>) - Force the deletion of the cargo
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The cargo has been deleted
///   - [Err](HttpError) - The cargo has not been deleted
///
pub async fn delete_by_key(
  key: &str,
  force: Option<bool>,
  state: &DaemonState,
) -> Result<(), HttpError> {
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
  repositories::cargo_config::delete_by_cargo_key(key, &state.pool).await?;
  Ok(())
}

/// ## Delete instances
///
/// The instances (containers) are deleted but the cargo is not.
/// The cargo is not deleted because it can be used to restore the containers.
///
/// ## Arguments
///
/// - [instances](Vec<ContainerSummary>) - The instances to delete
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been deleted
///   - [Err](HttpError) - The containers has not been deleted
///
async fn delete_instances(
  instances: &[String],
  state: &DaemonState,
) -> Result<(), HttpError> {
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

/// ## Put
///
/// A new history entry is added and the containers are updated
/// with the new cargo configuration
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [cargo_partial](CargoConfigPartial) - The cargo config
/// - [version](str) - The version of the api to use
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been patched
///   - [Err](HttpError) - The cargo has not been patched
///
pub async fn put(
  cargo_key: &str,
  cargo_partial: &CargoConfigPartial,
  version: &str,
  state: &DaemonState,
) -> Result<Cargo, HttpError> {
  let cargo = repositories::cargo::update_by_key(
    cargo_key,
    cargo_partial,
    version,
    &state.pool,
  )
  .await?;
  // Get the number of instance to create
  let number = if let Some(mode) = &cargo.config.replication {
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
  // Create instance with the new config
  let new_instances =
    match create_instances(&cargo, 0, number, &state.docker_api).await {
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
      log::error!("Unable to start cargo instance {} : {err}", cargo.key);
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
  Ok(cargo)
}

/// ## List
///
/// List the cargoes for the given query
///
/// ## Arguments
///
/// - [query](GenericCargoListQuery) - The filter query
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers of the cargo
///   - [Err](HttpError) - The containers of the cargo has not been listed
///
pub async fn list(
  query: GenericCargoListQuery<&str>,
  state: &DaemonState,
) -> Result<Vec<CargoSummary>, HttpError> {
  let namespace =
    repositories::namespace::find_by_name(query.namespace, &state.pool).await?;
  let query = query.merge(namespace);
  let cargoes = repositories::cargo::list_by_query(&query, &state.pool).await?;
  let mut cargo_summaries = Vec::new();
  let nodes =
    repositories::node::list_unless(&state.config.hostname, &state.pool)
      .await?;
  for cargo in cargoes {
    let config =
      repositories::cargo_config::find_by_key(&cargo.config_key, &state.pool)
        .await?;
    let mut containers = list_instances(&cargo.key, &state.docker_api).await?;
    for node in &nodes {
      let client = node.to_http_client();
      let node_containers = match client
        .list_cargo_instance(&cargo.name, Some(cargo.namespace_name.clone()))
        .await
      {
        Ok(containers) => containers,
        Err(err) => {
          log::error!(
            "Unable to list cargo instance on node {} : {}",
            node.name,
            err
          );
          continue;
        }
      };
      containers.extend(node_containers);
    }
    let mut running_instances = 0;
    for container in containers.clone() {
      if container.state == Some("running".into()) {
        running_instances += 1;
      }
    }
    cargo_summaries.push(CargoSummary {
      key: cargo.key,
      created_at: cargo.created_at,
      updated_at: config.created_at,
      name: cargo.name,
      namespace_name: cargo.namespace_name,
      config: config.to_owned(),
      instance_total: containers.len(),
      instance_running: running_instances,
      config_key: config.key,
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
/// - [key](str) - The cargo key
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoInspect) - The cargo information
///   - [Err](HttpError) - The cargo has not been inspected
///
pub async fn inspect_by_key(
  key: &str,
  state: &DaemonState,
) -> Result<CargoInspect, HttpError> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let containers = list_instances(&cargo.key, &state.docker_api).await?;
  let mut containers = containers
    .into_iter()
    .map(|c| {
      NodeContainerSummary::new(
        state.config.hostname.clone(),
        state.config.advertise_addr.clone(),
        c,
      )
    })
    .collect::<Vec<NodeContainerSummary>>();
  let nodes =
    repositories::node::list_unless(&state.config.hostname, &state.pool)
      .await?;
  for node in &nodes {
    let client = node.to_http_client();
    let node_containers = match client
      .list_cargo_instance(&cargo.name, Some(cargo.namespace_name.clone()))
      .await
    {
      Ok(containers) => containers,
      Err(err) => {
        log::error!(
          "Unable to list cargo instance on node {} : {}",
          node.name,
          err
        );
        continue;
      }
    };
    let node_containers = node_containers
      .into_iter()
      .map(|c| {
        NodeContainerSummary::new(node.name.clone(), node.ip_address.clone(), c)
      })
      .collect::<Vec<NodeContainerSummary>>();
    containers.extend(node_containers);
  }
  let mut running_instances = 0;
  for nc in &containers {
    if nc.container.state == Some("running".into()) {
      running_instances += 1;
    }
  }
  Ok(CargoInspect {
    key: cargo.key,
    name: cargo.name,
    config_key: cargo.config_key,
    namespace_name: cargo.namespace_name,
    config: cargo.config,
    instance_total: containers.len(),
    instance_running: running_instances,
    instances: containers,
  })
}

/// ## Delete by namespace
///
/// This remove all cargo in the given namespace and all their instances (containers)
/// from the system (database and docker).
///
/// ## Arguments
///
/// - [namespace](str) - The namespace name
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The cargoes has been deleted
///   - [Err](HttpError) - The cargo has not been deleted
///
pub async fn delete_by_namespace(
  namespace: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
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
/// - [name](str) - The cargo name
/// - [options](CargoKillOptions) - The kill options
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](()) - The signal has been sent
///  - [Err](HttpError) - The signal has not been sent
///
pub async fn kill_by_name(
  name: &str,
  options: &CargoKillOptions,
  docker_api: &bollard_next::Docker,
) -> Result<(), HttpError> {
  let name = format!("{name}.c");
  let options = options.clone().into();
  docker_api.kill_container(&name, Some(options)).await?;
  Ok(())
}

/// ## Patch
///
/// Merge the given cargo config with the existing one
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [payload](CargoConfigUpdate) - The cargo config update
/// - [version](str) - The cargo version
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Cargo) - The cargo has been patched
///  - [Err](HttpError) - The cargo has not been patched
///
pub async fn patch(
  key: &str,
  payload: &CargoConfigUpdate,
  version: &str,
  state: &DaemonState,
) -> Result<Cargo, HttpError> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let container = if let Some(container) = payload.container.clone() {
    // merge env and ensure no duplicate key
    let new_env = container.env.unwrap_or_default();
    let mut env_vars: Vec<String> =
      cargo.config.container.env.unwrap_or_default();
    // Merge environment variables from new_env into the merged array
    for env_var in new_env {
      let parts: Vec<&str> = env_var.split('=').collect();
      if parts.len() != 2 {
        continue;
      }
      let name = parts[0].to_string();
      let value = parts[1].to_string();
      if let Some(pos) = env_vars.iter().position(|x| x.starts_with(&name)) {
        let old_value = env_vars[pos].split('=').nth(1).unwrap().to_string();
        if old_value != value && !value.is_empty() {
          // Update the value if it has changed
          env_vars[pos] = format!("{}={}", name, value);
        } else if value.is_empty() {
          // Remove the variable if the value is empty
          env_vars.remove(pos);
        }
      } else {
        // Add new environment variables
        env_vars.push(env_var.to_string());
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
      .config
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
      cargo.config.container.image
    };
    let cmd = if let Some(cmd) = container.cmd {
      Some(cmd)
    } else {
      cargo.config.container.cmd
    };
    ContainerConfig {
      cmd,
      image,
      env: Some(env_vars),
      host_config: Some(HostConfig {
        binds: Some(volumes),
        ..cargo.config.container.host_config.unwrap_or_default()
      }),
      ..cargo.config.container
    }
  } else {
    cargo.config.container
  };
  let config = CargoConfigPartial {
    name: cargo.name.clone(),
    container,
    replication: payload.replication.clone(),
  };
  utils::cargo::put(key, &config, version, state).await
}

/// ## Get logs
///
/// Get the logs of a cargo instance
/// The cargo name can be used if the cargo has only one instance
/// The query parameter can be used to filter the logs
///
/// ## Arguments
///
/// - [name](str): The cargo name
/// - [query](CargoLogQuery): The query parameters
/// - [docker_api](bollard_next::Docker): The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Stream) - The stream of logs
///   - [Err](HttpError) - The logs could not be retrieved
///
pub fn get_logs(
  name: &str,
  query: &CargoLogQuery,
  docker_api: &bollard_next::Docker,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
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
/// - [name](str): The cargo name
/// - [query](CargoStatsQuery): The query parameters
/// - [docker_api](bollard_next::Docker): The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Stream) - The stream of logs
///   - [Err](HttpError) - The logs could not be retrieved
///
pub fn get_stats(
  name: &str,
  query: &CargoStatsQuery,
  docker_api: &bollard_next::Docker,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
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
/// - [key](str) - The cargo key
/// - [options](CargoScale) - The scale options
/// - [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The cargo has been scaled
///   - [Err](HttpError) - The cargo has not been scaled
///
pub async fn scale(
  key: &str,
  options: &CargoScale,
  state: &DaemonState,
) -> Result<(), HttpError> {
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
      create_instances(&cargo, instances.len(), to_add, &state.docker_api)
        .await?;
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
  Ok(())
}
