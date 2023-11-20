use std::collections::HashMap;

use ntex::http;

use bollard_next::models::ContainerSummary;
use bollard_next::container::ListContainersOptions;
use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespaceInspect, NamespacePartial,
  NamespaceListQuery,
};

use crate::{utils, repositories};
use crate::models::{Pool, DaemonState};

use super::cargo;

/// ## Create
///
/// Create a new namespace with his associated network.
/// Each vm and cargo created on this namespace will use the same network.
///
/// ## Arguments
///
/// * [item](NamespacePartial) - The namespace to create
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Namespace](Namespace)
///
pub(crate) async fn create(
  item: &NamespacePartial,
  state: &DaemonState,
) -> HttpResult<Namespace> {
  if repositories::namespace::exist_by_name(&item.name, &state.pool).await? {
    return Err(HttpError {
      msg: format!("namespace {} error: already exist", &item.name),
      status: http::StatusCode::CONFLICT,
    });
  }
  if state
    .docker_api
    .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
    .await
    .is_ok()
  {
    let res = repositories::namespace::create(item, &state.pool).await?;
    return Ok(Namespace { name: res.name });
  }
  let config = CreateNetworkOptions {
    name: item.name.to_owned(),
    driver: String::from("bridge"),
    ..Default::default()
  };
  state.docker_api.create_network(config).await?;
  let res = repositories::namespace::create(item, &state.pool).await?;
  Ok(Namespace { name: res.name })
}

/// ## Delete by name
///
/// Delete a namespace by name and remove all associated cargo and vm.
///
/// ## Arguments
///
/// * [name](str) - The namespace name
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<GenericDelete> {
  utils::cargo::delete_by_namespace(name, state).await?;
  if let Err(err) = state.docker_api.remove_network(name).await {
    log::error!("Unable to remove network {} got error: {}", name, err);
  }
  let res = repositories::namespace::delete_by_name(name, &state.pool).await?;
  Ok(res)
}

/// ## List instances
///
/// List all instances on a namespace
///
/// ## Arguments
///
/// * [namespace](str) - The namespace
/// * [docker_api](bollard_next::Docker) - The docker api
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [ContainerSummary](ContainerSummary)
///
pub(crate) async fn list_instances(
  namespace: &str,
  docker_api: &bollard_next::Docker,
) -> HttpResult<Vec<ContainerSummary>> {
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

/// ## List
///
/// List all existing namespaces
///
/// ## Arguments
///
/// * [docker_api](bollard_next::Docker) - The docker api
/// * [pool](Pool) - The database pool
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Vec](Vec) of [NamespaceSummary](NamespaceSummary)
///
pub(crate) async fn list(
  query: &NamespaceListQuery,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> HttpResult<Vec<NamespaceSummary>> {
  let items = repositories::namespace::list(query, pool).await?;
  let mut new_items = Vec::new();
  for item in items {
    let cargo_count =
      repositories::cargo::count_by_namespace(&item.name, pool).await?;
    let instance_count = list_instances(&item.name, docker_api).await?.len();
    let network = docker_api
      .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
      .await?;
    let ipam = network.ipam.unwrap_or_default();
    let ipam_config = ipam.config.unwrap_or_default();
    let gateway = ipam_config
      .get(0)
      .ok_or(HttpError {
        msg: format!("Unable to get gateway for network {}", &item.name),
        status: http::StatusCode::INTERNAL_SERVER_ERROR,
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

/// ## Inspect by name
///
/// Get detailed information about a namespace
///
/// ## Arguments
///
/// * [name](str) - The namespace name
/// * [state](DaemonState) - The daemon state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [NamespaceInspect](NamespaceInspect)
///
pub(crate) async fn inspect_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<NamespaceInspect> {
  let namespace =
    repositories::namespace::find_by_name(name, &state.pool).await?;
  log::debug!("Found namespace to inspect {:?}", &namespace);
  let cargo_db_models =
    repositories::cargo::find_by_namespace(&namespace, &state.pool).await?;
  log::debug!("Found namespace cargoes to inspect {:?}", &cargo_db_models);
  let mut cargoes = Vec::new();
  for cargo in cargo_db_models {
    let cargo = cargo::inspect_by_key(&cargo.key, state).await?;
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

/// ## Create if not exists
///
/// Create a namespace if it does not exists
///
/// ## Arguments
///
/// * [name](str) - The namespace name
/// * [state](DaemonState) - The daemon state
///
pub(crate) async fn create_if_not_exists(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
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
