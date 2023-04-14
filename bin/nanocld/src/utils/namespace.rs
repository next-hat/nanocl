use std::collections::HashMap;

use ntex::http::StatusCode;

use bollard_next::models::ContainerSummary;
use bollard_next::container::ListContainersOptions;
use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespaceInspect, NamespacePartial,
};

use crate::models::DaemonState;
use crate::utils;
use crate::repositories;
use crate::models::Pool;
use crate::error::HttpError;

use super::cargo;

/// ## Create a namespace
///
/// Create a new namespace with his associated network
///
/// ## Arguments
///
/// - [namespace](NamespacePartial) - The namespace name
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](Namespace) - The namespace has been created
///  - [Err](HttpResponseError) - The namespace has not been created
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::create("my-namespace", &docker_api, &pool).await;
/// ```
///
pub async fn create(
  namespace: &NamespacePartial,
  state: &DaemonState,
) -> Result<Namespace, HttpError> {
  if repositories::namespace::exist_by_name(&namespace.name, &state.pool)
    .await?
  {
    return Err(HttpError {
      msg: format!("namespace {} error: already exist", &namespace.name),
      status: StatusCode::CONFLICT,
    });
  }
  if state
    .docker_api
    .inspect_network(&namespace.name, None::<InspectNetworkOptions<String>>)
    .await
    .is_ok()
  {
    let res = repositories::namespace::create(namespace, &state.pool).await?;
    return Ok(Namespace { name: res.name });
  }
  let config = CreateNetworkOptions {
    name: namespace.name.to_owned(),
    driver: String::from("bridge"),
    ..Default::default()
  };
  state.docker_api.create_network(config).await?;
  let res = repositories::namespace::create(namespace, &state.pool).await?;
  Ok(Namespace { name: res.name })
}

/// ## Remove a namespace
///
/// Remove a namespace and his associated network with all his cargoes
///
/// ## Arguments
///
/// - [name](String) - The namespace name
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](GenericDelete) - The namespace has been removed
///   - [Err](HttpResponseError) - The namespace has not been removed
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::delete_by_name("my-namespace", &docker_api, &pool).await;
/// ```
///
pub async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> Result<GenericDelete, HttpError> {
  utils::cargo::delete_by_namespace(name, state).await?;
  if let Err(err) = state.docker_api.remove_network(name).await {
    log::error!("Unable to remove network {} got error: {}", name, err);
  }
  repositories::namespace::delete_by_name(name, &state.pool).await
}

/// ## List existing container in a namespace
///
/// List containers based on the namespace
///
/// ## Arguments
///
/// - [namespace](String) - The namespace
/// - [docker_api](bollard_next::Docker) - The docker api
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<ContainerSummary>) - The containers have been listed
///   - [Err](HttpResponseError) - The containers have not been listed
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::list_instance("my-namespace", &docker_api).await;
/// ```
///
pub async fn list_instance(
  namespace: &str,
  docker_api: &bollard_next::Docker,
) -> Result<Vec<ContainerSummary>, HttpError> {
  let label = format!("io.nanocl.n={namespace}");
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

/// ## List namespaces
///
/// List all existing namespaces
///
/// ## Arguments
///
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Vec<NamespaceSummary>) - The namespaces have been listed
///   - [Err](HttpResponseError) - The namespaces have not been listed
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::list(&docker_api, &pool).await;
/// ```
///
pub async fn list(
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> Result<Vec<NamespaceSummary>, HttpError> {
  let items = repositories::namespace::list(pool).await?;
  let mut new_items = Vec::new();
  for item in items {
    let cargo_count =
      repositories::cargo::count_by_namespace(&item.name, pool).await?;
    let instance_count = list_instance(&item.name, docker_api).await?.len();

    let network = docker_api
      .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
      .await?;
    let ipam = network.ipam.unwrap_or_default();
    let ipam_config = ipam.config.unwrap_or_default();

    let gateway = ipam_config
      .get(0)
      .ok_or(HttpError {
        msg: format!("Unable to get gateway for network {}", &item.name),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      })?
      .gateway
      .clone()
      .unwrap_or_default();

    new_items.push(NamespaceSummary {
      name: item.name.to_owned(),
      cargoes: cargo_count,
      instances: instance_count.try_into().unwrap(),
      gateway,
    })
  }
  Ok(new_items)
}

/// ## Inspect a namespace
///
/// Get detailed information about a namespace
///
/// ## Arguments
///
/// - [namespace](String) - The namespace
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](NamespaceInspect) - The namespace has been inspected
///   - [Err](HttpResponseError) - The namespace has not been inspected
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::inspect("my-namespace", &docker_api, &pool).await;
/// ```
///
pub async fn inspect(
  name: &str,
  state: &DaemonState,
) -> Result<NamespaceInspect, HttpError> {
  let namespace =
    repositories::namespace::find_by_name(name, &state.pool).await?;
  log::debug!("Found namespace to inspect {:?}", &namespace);
  let cargo_db_models =
    repositories::cargo::find_by_namespace(&namespace, &state.pool).await?;
  log::debug!("Found namespace cargoes to inspect {:?}", &cargo_db_models);
  let mut cargoes = Vec::new();
  for cargo in cargo_db_models {
    let cargo = cargo::inspect(&cargo.key, state).await?;
    cargoes.push(cargo);
  }
  let network = state
    .docker_api
    .inspect_network(name, None::<InspectNetworkOptions<String>>)
    .await?;
  Ok(NamespaceInspect {
    name: namespace.name,
    cargoes,
    network,
  })
}

/// ## Create a namespace if not exists
///
///
/// ## Arguments
///
/// - [name](String) - The namespace name
/// - [docker_api](bollard_next::Docker) - The docker api
/// - [pool](Pool) - The database pool
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///  - [Ok](()) - The namespace will exists
///  - [Err](HttpResponseError) - An error occured
///
/// ## Example
///
/// ```rust,norun
/// use bollard_next::Docker;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let result = namespace::create_if_not_exists("my-namespace", &docker_api, &pool).await;
/// ```
///
pub async fn create_if_not_exists(
  name: &str,
  state: &DaemonState,
) -> Result<(), HttpError> {
  if repositories::namespace::find_by_name(name, &state.pool)
    .await
    .is_err()
  {
    create(
      &NamespacePartial {
        name: name.to_owned(),
      },
      state,
    )
    .await?;
  }
  Ok(())
}
