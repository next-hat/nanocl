use std::collections::HashMap;

use ntex::rt;
use ntex::web;
use ntex::util::Bytes;
use ntex::http::StatusCode;
use futures::{StreamExt, TryStreamExt};

use bollard_next::container::LogOutput;
use bollard_next::container::LogsOptions;
use nanocl_stubs::node::NodeContainerSummary;
use bollard_next::container::WaitContainerOptions;
use bollard_next::exec::{StartExecOptions, StartExecResults};
use bollard_next::service::{ContainerSummary, HostConfig};
use bollard_next::service::{RestartPolicy, RestartPolicyNameEnum};
use bollard_next::container::{ListContainersOptions, RemoveContainerOptions};

use nanocl_stubs::cargo::{CargoKillOptions, GenericCargoListQuery};
use nanocl_stubs::cargo_config::Config as ContainerConfig;
use nanocl_stubs::cargo_config::{CargoConfigPartial, CargoConfigUpdate};
use nanocl_stubs::cargo::{
  Cargo, CargoSummary, CargoInspect, OutputLog, CreateExecOptions, CargoLogQuery
};

use crate::models::DaemonState;
use crate::{utils, repositories};
use nanocl_utils::http_error::HttpError;

use super::stream::transform_stream;

/// ## Create instance
///
/// Create containers based on the cargo config
/// The number of containers created is based on the number of instances
/// defined in the cargo config
/// If the number of instances is greater than 1, the containers will be named
/// with the cargo key and a number
/// Example: cargo-key-1, cargo-key-2, cargo-key-3
/// If the number of instances is equal to 1, the container will be named with
/// the cargo key
/// Example: cargo-key
/// The cargo key is used to track the containers
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
///   - [Ok](Ok) - The containers has been created
///   - [Err](HttpResponseError) - The containers has not been created
///
async fn create_instance(
  cargo: &Cargo,
  number: i64,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<String>, HttpError> {
  let mut instances = Vec::new();
  for current in 0..number {
    let name = if current > 0 {
      format!("{}-{}.c", cargo.key, current)
    } else {
      format!("{}.c", cargo.key)
    };

    let create_options = bollard_next::container::CreateContainerOptions {
      name,
      ..Default::default()
    };

    // Add cargo label to the container to track it
    let mut labels =
      cargo.config.container.labels.to_owned().unwrap_or_default();
    labels.insert("io.nanocl".into(), "enabled".into());
    labels.insert("io.nanocl.c".into(), cargo.key.to_owned());
    labels.insert("io.nanocl.n".into(), cargo.namespace_name.to_owned());
    labels.insert("io.nanocl.cnsp".into(), cargo.namespace_name.to_owned());

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

    // Merge the cargo config with the container config
    // And set his network mode to the cargo namespace
    let config = bollard_next::container::Config {
      attach_stderr: Some(true),
      attach_stdout: Some(true),
      tty: Some(true),
      labels: Some(labels),
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
          .to_owned()
      }),
      ..cargo.config.container.to_owned()
    };

    let res = docker_api
      .create_container::<String>(Some(create_options), config)
      .await
      .map_err(|e| HttpError {
        msg: format!("Unable to create container: {e}"),
        status: StatusCode::BAD_REQUEST,
      })?;
    instances.push(res.id);
  }

  Ok(instances)
}

/// ## List containers based on the cargo key
///
/// ## Arguments
///
/// - [cargo_key](String) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers have been listed
///   - [Err](HttpResponseError) - The containers have not been listed
///
pub async fn list_instance(
  cargo_key: &str,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<ContainerSummary>, HttpError> {
  let label = format!("io.nanocl.c={cargo_key}");
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

/// ## Create a new cargo with his containers
///
/// ## Arguments
///
/// - [cargo_partial](CargoConfigPartial) - The cargo partial
/// - [namespace](String) - The namespace
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been created
///   - [Err](HttpResponseError) - The cargo has not been created
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

  if let Err(err) = create_instance(&cargo, 1, &state.docker_api).await {
    repositories::cargo::delete_by_key(&cargo.key, &state.pool).await?;
    return Err(err);
  }

  Ok(cargo)
}

/// ## Start containers of the given cargo
///
/// The containers are started in parallel
/// If one container fails to start, the other containers will continue to start
///
/// ## Arguments
///
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The containers has been started
///   - [Err](HttpResponseError) - The containers has not been started
///
pub async fn start(
  cargo_key: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let cargo_key = cargo_key.to_owned();
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

  let containers = list_instance(&cargo_key, &docker_api).await?;

  let mut futures = Vec::new();

  for container in containers {
    let id = container.id.unwrap_or_default();

    if auto_remove {
      let id = id.clone();
      let docker_api = docker_api.clone();
      futures.push(async move {
        let options = Some(WaitContainerOptions {
          condition: "removed",
        });
        let stream = docker_api.wait_container(&id, options);
        if let Err(err) = stream.try_for_each(|_| async { Ok(()) }).await {
          log::warn!("Error while waiting for container {id} {err}");
        }
      });
    }
    docker_api.start_container::<String>(&id, None).await?;
  }

  if auto_remove {
    let pool = state.pool.clone();
    rt::spawn(async move {
      futures::future::join_all(futures).await;
      if let Err(err) =
        repositories::cargo::delete_by_key(&cargo_key, &pool).await
      {
        log::warn!("Error while deleting cargo {cargo_key} {err}");
      }
    });
  }

  Ok(())
}

/// ## Stop containers of the given cargo
///
/// ## Arguments
///
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Ok) - The containers has been stopped
///  - [Err](HttpResponseError) - The containers has not been stopped
///
pub async fn stop(
  cargo_key: &str,
  docker_api: &bollard_next::Docker,
) -> Result<(), HttpError> {
  let containers = list_instance(cargo_key, docker_api).await?;

  for container in containers {
    docker_api
      .stop_container(&container.id.unwrap_or_default(), None)
      .await?;
  }

  Ok(())
}

/// Delete containers of the given cargo and the cargo itself
///
/// ## Arguments
///
/// - [cargo_key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Ok) - The cargo has been deleted
///   - [Err](HttpResponseError) - The cargo has not been deleted
///
pub async fn delete(
  cargo_key: &str,
  force: Option<bool>,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let containers = list_instance(cargo_key, &state.docker_api).await?;

  for container in containers {
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
      .map_err(|e| HttpError {
        msg: format!("Unable to remove container got error : {e}"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  repositories::cargo::delete_by_key(cargo_key, &state.pool).await?;
  repositories::cargo_config::delete_by_cargo_key(cargo_key, &state.pool)
    .await?;

  Ok(())
}

/// Patch a cargo
/// The cargo is patched and the containers are updated
/// The containers are updated in parallel
/// If one container fails to update, the other containers will continue to update
/// The containers are updated with the new cargo configuration
///
/// ## Arguments
/// - [cargo_key](str) - The cargo key
/// - [cargo_partial](CargoPartial) - The cargo partial
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
/// - [Result](Result) - The result of the operation
///   - [Ok](Cargo) - The cargo has been patched
///   - [Err](HttpResponseError) - The cargo has not been patched
///
pub async fn put(
  cargo_key: &str,
  config: &CargoConfigUpdate,
  version: &str,
  state: &DaemonState,
) -> Result<Cargo, HttpError> {
  let cargo = repositories::cargo::find_by_key(cargo_key, &state.pool).await?;

  let cargo_config =
    repositories::cargo_config::find_by_key(&cargo.config_key, &state.pool)
      .await?;

  // Todo should remove this as part of a patch
  let cargo_partial = CargoConfigPartial {
    name: config.name.to_owned().unwrap_or(cargo.name),
    container: config
      .container
      .to_owned()
      .unwrap_or(cargo_config.container),
    replication: if config.replication.is_some() {
      config.replication.to_owned()
    } else {
      cargo_config.replication.to_owned()
    },
  };

  let cargo = repositories::cargo::update_by_key(
    cargo_key,
    &cargo_partial,
    version,
    &state.pool,
  )
  .await?;

  let containers = list_instance(cargo_key, &state.docker_api).await?;

  // Rename existing container to avoid name conflict
  for container in containers.iter().cloned() {
    let names = container.names.unwrap_or_default();
    let name = format!("{}-backup", names[0]);

    state
      .docker_api
      .rename_container(
        &container.id.unwrap_or_default(),
        bollard_next::container::RenameContainerOptions { name },
      )
      .await
      .map_err(|e| HttpError {
        msg: format!("Unable to rename container got error : {e}"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  // Create instance with the new config
  let new_instances = match create_instance(&cargo, 1, &state.docker_api).await
  {
    Err(err) => {
      // If the creation of the new instance failed, we rename the old containers
      for container in containers.iter().cloned() {
        let names = container.names.unwrap_or_default();
        let name = names[0].replace("-backup", "");

        state
          .docker_api
          .rename_container(
            &container.id.unwrap_or_default(),
            bollard_next::container::RenameContainerOptions { name },
          )
          .await
          .map_err(|e| HttpError {
            msg: format!("Unable to rename container got error : {e}"),
            status: StatusCode::INTERNAL_SERVER_ERROR,
          })?;
        log::error!("Unable to create cargo instance {} : {err}", cargo.key);
      }
      Vec::new()
    }
    Ok(instances) => instances,
  };

  // start created containers
  if let Err(err) = start(cargo_key, state).await {
    log::error!("Unable to start cargo instance {} : {err}", cargo.key);
    // If the start of the new instance failed, we remove the new containers
    for instance in new_instances {
      state
        .docker_api
        .remove_container(
          &instance,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
        )
        .await?;
    }
    // We rename the old containers
    for container in containers.iter().cloned() {
      let names = container.names.unwrap_or_default();
      let name = names[0].replace("-backup", "");

      state
        .docker_api
        .rename_container(
          &container.id.unwrap_or_default(),
          bollard_next::container::RenameContainerOptions { name },
        )
        .await?;
    }
  }

  // Delete old cors
  for container in containers {
    state
      .docker_api
      .remove_container(
        &container.id.unwrap_or_default(),
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
      )
      .await
      .map_err(|e| HttpError {
        msg: format!("Unable to remove container got error : {e}"),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?;
  }

  Ok(cargo)
}

/// ## List cargo in given namespace
///
/// The containers are filtered by the cargo key
///
/// ## Arguments
///
/// - [query](GenericCargoListQuery) - The query containing namespace, name filter and
/// pagination info
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers of the cargo
///   - [Err](HttpResponseError) - The containers of the cargo has not been listed
///
pub async fn list(
  query: GenericCargoListQuery<&str>,
  state: &DaemonState,
) -> Result<Vec<CargoSummary>, HttpError> {
  let namespace =
    repositories::namespace::find_by_name(query.namespace, &state.pool).await?;

  let query = query.merge(namespace);
  let cargoes =
    repositories::cargo::find_by_list_query(&query, &state.pool).await?;

  let mut cargo_summaries = Vec::new();

  let nodes =
    repositories::node::list_unless(&state.config.hostname, &state.pool)
      .await?;

  for cargo in cargoes {
    let config =
      repositories::cargo_config::find_by_key(&cargo.config_key, &state.pool)
        .await?;
    let mut containers = list_instance(&cargo.key, &state.docker_api).await?;

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

/// ## Inspect cargo
///
/// Return information about the cargo
///
/// ## Arguments
///
/// - [key](str) - The cargo key
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoInspect) - The cargo information
///   - [Err](HttpResponseError) - The cargo has not been inspected
///
pub async fn inspect(
  key: &str,
  state: &DaemonState,
) -> Result<CargoInspect, HttpError> {
  let cargo = repositories::cargo::inspect_by_key(key, &state.pool).await?;
  let containers = list_instance(&cargo.key, &state.docker_api).await?;

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

/// ## Delete all cargoes in given namespace
///
/// ## Arguments
///
/// - [namespace](str) - The namespace name
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The cargoes has been deleted
///   - [Err](HttpResponseError) - The cargo has not been deleted
///
/// ## Example
///
/// ```rust,norun
/// use crate::repositories;
///
/// let namespace = "my-namespace";
/// let docker_api = bollard_next::Docker::connect_with_local_defaults().unwrap();
/// repositories::cargo::delete_by_namespace(namespace, &docker_api, &pool).await?;
/// ```
///
pub async fn delete_by_namespace(
  namespace: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let namespace =
    repositories::namespace::find_by_name(namespace, &state.pool).await?;

  let cargoes =
    repositories::cargo::find_by_namespace(&namespace, &state.pool).await?;

  for cargo in cargoes {
    delete(&cargo.key, None, state).await?;
  }

  Ok(())
}

/// ## Exec command
///
/// Execute a command in a container the cargo name can be used if the cargo has only one instance
///
pub async fn exec_command(
  name: &str,
  args: &CreateExecOptions,
  state: &DaemonState,
) -> Result<web::HttpResponse, HttpError> {
  let name = format!("{name}.c");
  let result = state.docker_api.create_exec(&name, args.to_owned()).await?;

  let res = state
    .docker_api
    .start_exec(&result.id, Some(StartExecOptions::default()))
    .await?;

  match res {
    StartExecResults::Detached => Ok(web::HttpResponse::Ok().finish()),
    StartExecResults::Attached { output, .. } => {
      let stream = transform_stream::<LogOutput, OutputLog>(output);
      Ok(
        web::HttpResponse::Ok()
          .content_type("nanocl/streaming-v1")
          .streaming(stream),
      )
    }
  }
}

pub async fn kill(
  name: &str,
  options: &CargoKillOptions,
  docker_api: &bollard_next::Docker,
) -> Result<(), HttpError> {
  let name = format!("{name}.c");
  let options = options.clone().into();
  docker_api.kill_container(&name, Some(options)).await?;
  Ok(())
}

/// ## Create or patch cargo
///
/// Create a cargo if it does not exist or patch it if it exists
///
/// ## Arguments
///
/// - [namespace](str) - The namespace name
/// - [cargo](CargoConfigPartial) - The cargo config
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The cargo has been created or patched
///   - [Err](HttpResponseError) - The cargo has not been created or patched
///
pub async fn create_or_put(
  namespace: &str,
  cargo: &CargoConfigPartial,
  version: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  let key = utils::key::gen_key(namespace, &cargo.name);
  match utils::cargo::inspect(&key, state).await {
    Ok(existing) => {
      let existing: CargoConfigPartial = existing.into();
      if existing == *cargo {
        log::debug!("No changes detected for cargo {} skipping", &key);
        return Ok(());
      }
      utils::cargo::put(&key, &cargo.clone().into(), version, state).await?;
    }
    Err(_err) => {
      utils::cargo::create(namespace, cargo, version, state).await?;
      utils::cargo::start(&key, state).await?;
    }
  }
  Ok(())
}

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

  let config = CargoConfigUpdate {
    container: Some(container),
    ..payload.to_owned()
  };
  utils::cargo::put(key, &config, version, state).await
}

pub fn get_logs(
  name: &str,
  query: &CargoLogQuery,
  docker_api: &bollard_next::Docker,
) -> Result<impl StreamExt<Item = Result<Bytes, HttpError>>, HttpError> {
  let stream = docker_api.logs(
    &format!("{name}.c"),
    Some(query.clone().into()),
  );
  let stream = transform_stream::<LogOutput, OutputLog>(stream);
  Ok(stream)
}
