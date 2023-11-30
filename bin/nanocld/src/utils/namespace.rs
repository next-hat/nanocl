use std::collections::HashMap;

use ntex::http;

use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::models::ContainerSummary;
use bollard_next::container::ListContainersOptions;
use bollard_next::network::{CreateNetworkOptions, InspectNetworkOptions};
use nanocl_stubs::generic::GenericFilter;
use nanocl_stubs::namespace::{
  Namespace, NamespaceSummary, NamespaceInspect, NamespacePartial,
  NamespaceListQuery,
};

use crate::utils;
use crate::models::{Pool, DaemonState, CargoDb, NamespaceDb, Repository};

/// Create a new namespace with his associated network.
/// Each vm and cargo created on this namespace will use the same network.
pub(crate) async fn create(
  item: &NamespacePartial,
  state: &DaemonState,
) -> HttpResult<Namespace> {
  if NamespaceDb::find_by_pk(&item.name, &state.pool)
    .await?
    .is_ok()
  {
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
    let res = NamespaceDb::create(item, &state.pool).await??;
    return Ok(Namespace { name: res.name });
  }
  let config = CreateNetworkOptions {
    name: item.name.to_owned(),
    driver: String::from("bridge"),
    ..Default::default()
  };
  state.docker_api.create_network(config).await?;
  let res = NamespaceDb::create(item, &state.pool).await??;
  Ok(Namespace { name: res.name })
}

/// Delete a namespace by name and remove all associated cargo and vm.
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  utils::cargo::delete_by_namespace(name, state).await?;
  NamespaceDb::delete_by_pk(name, &state.pool).await??;
  if let Err(err) = state.docker_api.remove_network(name).await {
    log::error!("Unable to remove network {} got error: {}", name, err);
  }
  Ok(())
}

/// List all instances on a namespace
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

/// List all existing namespaces
pub(crate) async fn list(
  query: &NamespaceListQuery,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> HttpResult<Vec<NamespaceSummary>> {
  let items = NamespaceDb::find(&GenericFilter::default(), pool).await??;
  let mut new_items = Vec::new();
  for item in items {
    let cargo_count = CargoDb::count_by_namespace(&item.name, pool).await?;
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

/// Get detailed information about a namespace
pub(crate) async fn inspect_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<NamespaceInspect> {
  let namespace = NamespaceDb::find_by_pk(name, &state.pool).await??;
  log::debug!("Found namespace to inspect {:?}", &namespace);
  let models = CargoDb::find_by_namespace(&namespace.name, &state.pool).await?;
  log::debug!("Found namespace cargoes to inspect {:?}", &models);
  let mut cargoes = Vec::new();
  for cargo in models {
    let cargo =
      utils::cargo::inspect_by_key(&cargo.spec.cargo_key, state).await?;
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

/// Create a namespace if it does not exists
pub(crate) async fn create_if_not_exists(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  if NamespaceDb::find_by_pk(name, &state.pool).await?.is_err() {
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
