use nanocl_error::http::{HttpError, HttpResult};

use bollard_next::network::InspectNetworkOptions;
use nanocl_stubs::{generic::GenericFilter, namespace::NamespaceSummary};

use crate::{
  utils,
  repositories::generic::*,
  models::{CargoDb, NamespaceDb, SystemState},
};

/// List all existing namespaces
pub async fn list(
  filter: &GenericFilter,
  state: &SystemState,
) -> HttpResult<Vec<NamespaceSummary>> {
  let items = NamespaceDb::read_by(filter, &state.pool).await?;
  let mut new_items = Vec::new();
  for item in items {
    let cargo_count =
      CargoDb::count_by_namespace(&item.name, &state.pool).await?;
    let processes =
      utils::process::list_by_namespace(&item.name, state).await?;
    let network = state
      .docker_api
      .inspect_network(&item.name, None::<InspectNetworkOptions<String>>)
      .await?;
    let ipam = network.ipam.unwrap_or_default();
    let ipam_config = ipam.config.unwrap_or_default();
    let gateway = ipam_config
      .first()
      .ok_or(HttpError::internal_server_error(format!(
        "Unable to get gateway for network {}",
        &item.name
      )))?
      .gateway
      .clone()
      .unwrap_or_default();
    new_items.push(NamespaceSummary {
      name: item.name.to_owned(),
      cargoes: cargo_count as usize,
      instances: processes.len(),
      gateway,
      created_at: item.created_at,
    })
  }
  Ok(new_items)
}
