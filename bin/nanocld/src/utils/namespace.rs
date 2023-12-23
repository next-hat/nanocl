use std::collections::HashMap;

use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::{
  models::ContainerSummary,
  container::ListContainersOptions,
  network::{CreateNetworkOptions, InspectNetworkOptions},
};
use nanocl_stubs::{
  generic::GenericFilter,
  namespace::{
    Namespace, NamespaceSummary, NamespaceInspect, NamespacePartial,
  },
};

use crate::{
  utils,
  repositories::generic::*,
  models::{Pool, DaemonState, CargoDb, NamespaceDb},
};

/// Create a new namespace with his associated network.
/// Each vm and cargo created on this namespace will use the same network.
pub(crate) async fn create(
  item: &NamespacePartial,
  state: &DaemonState,
) -> HttpResult<Namespace> {
  if NamespaceDb::read_by_pk(&item.name, &state.pool)
    .await?
    .is_ok()
  {
    return Err(HttpError::conflict(format!(
      "Namespace {}: already exist",
      &item.name
    )));
  }
  if state
    .docker_api
    .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
    .await
    .is_ok()
  {
    let res = NamespaceDb::create_from(item, &state.pool).await??;
    return Ok(Namespace { name: res.name });
  }
  let config = CreateNetworkOptions {
    name: item.name.to_owned(),
    driver: String::from("bridge"),
    ..Default::default()
  };
  state.docker_api.create_network(config).await?;
  let res = NamespaceDb::create_from(item, &state.pool).await??;
  Ok(Namespace { name: res.name })
}

/// Delete a namespace by name and remove all associated cargo and vm.
pub(crate) async fn delete_by_name(
  name: &str,
  state: &DaemonState,
) -> HttpResult<()> {
  utils::cargo::delete_by_namespace(name, state).await?;
  NamespaceDb::del_by_pk(name, &state.pool).await??;
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
  filter: &GenericFilter,
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> HttpResult<Vec<NamespaceSummary>> {
  let items = NamespaceDb::read(filter, pool).await??;
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
      .ok_or(HttpError::internal_server_error(format!(
        "Unable to get gateway for network {}",
        &item.name
      )))?
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
  let namespace = NamespaceDb::read_by_pk(name, &state.pool).await??;
  let models = CargoDb::find_by_namespace(&namespace.name, &state.pool).await?;
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
