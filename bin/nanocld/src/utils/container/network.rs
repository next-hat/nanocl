use std::{
  collections::{BTreeSet, HashMap},
  sync::OnceLock,
};

use bollard_next::{
  container::Config,
  errors::Error as DockerError,
  network::{CreateNetworkOptions, InspectNetworkOptions, ListNetworksOptions},
};
use futures::lock::Mutex;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  network::NetworkPartial,
};

use crate::{
  models::{NetworkDb, SystemState},
  repositories::generic::{RepositoryDelBy, RepositoryDelByPk},
  utils,
};

pub const DEFAULT_NETWORK: &str = "nanoclbr0";

fn lifecycle_lock() -> &'static Mutex<()> {
  static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| Mutex::new(()))
}

/// Return whether a Docker network mode names a user-defined network.
///
/// Docker's built-in modes and container namespace sharing must never be
/// interpreted as network names to create.
pub fn is_custom_network(mode: &str) -> bool {
  !matches!(mode, "" | "default" | "bridge" | "host" | "none")
    && !mode.starts_with("container:")
}

/// Keep an explicit Docker network mode, or use Nanocl's bridge when omitted.
pub fn resolve_network_mode(mode: Option<String>) -> String {
  mode.unwrap_or_else(|| DEFAULT_NETWORK.to_owned())
}

/// Resolve the effective network mode of a Docker container configuration.
pub fn container_network_mode(container: &Config) -> String {
  resolve_network_mode(
    container
      .host_config
      .as_ref()
      .and_then(|host_config| host_config.network_mode.clone()),
  )
}

fn custom_network_modes(
  network_modes: impl IntoIterator<Item = String>,
) -> BTreeSet<String> {
  network_modes
    .into_iter()
    .filter(|network_mode| is_custom_network(network_mode))
    .collect()
}

fn docker_error(err: DockerError, context: String) -> IoError {
  *err.map_err_context(|| context)
}

fn has_status(err: &DockerError, expected: u16) -> bool {
  matches!(
    err,
    DockerError::DockerResponseServerError { status_code, .. }
      if *status_code == expected
  )
}

async fn inspect_network(
  name: &str,
  state: &SystemState,
) -> Result<bollard_next::service::Network, DockerError> {
  state
    .inner
    .docker_api
    .inspect_network(name, None::<InspectNetworkOptions<String>>)
    .await
}

async fn persist_network(
  requested_name: &str,
  network: bollard_next::service::Network,
  state: &SystemState,
) -> IoResult<String> {
  let node_name = state.inner.config.hostname.clone();
  let name = network
    .name
    .clone()
    .filter(|name| !name.is_empty())
    .unwrap_or_else(|| requested_name.to_owned());
  let network = NetworkPartial {
    key: utils::key::gen_network_key(&node_name, &name),
    name: name.clone(),
    node_name,
    data: network,
    metadata: None,
  };
  NetworkDb::upsert(&network, &state.inner.pool).await?;
  Ok(name)
}

async fn remove_network_record(
  network_name: &str,
  state: &SystemState,
) -> IoResult<()> {
  if !is_custom_network(network_name) {
    return Ok(());
  }
  let key =
    utils::key::gen_network_key(&state.inner.config.hostname, network_name);
  NetworkDb::del_by_pk(&key, &state.inner.pool).await
}

#[derive(Debug, PartialEq, Eq)]
enum MissingNetworkAction<'a> {
  ReinspectByName(&'a str),
  RemoveRecord(&'a str),
  Ignore,
}

fn missing_network_action<'a>(
  docker_reference: &str,
  network_name: Option<&'a str>,
) -> MissingNetworkAction<'a> {
  let Some(network_name) = network_name.filter(|name| is_custom_network(name))
  else {
    return MissingNetworkAction::Ignore;
  };
  if network_name == docker_reference {
    MissingNetworkAction::RemoveRecord(network_name)
  } else {
    MissingNetworkAction::ReinspectByName(network_name)
  }
}

async fn persist_inspected_network(
  requested_name: &str,
  network: bollard_next::service::Network,
  state: &SystemState,
) -> IoResult<Option<String>> {
  let name = network
    .name
    .as_deref()
    .filter(|name| !name.is_empty())
    .unwrap_or(requested_name);
  if !is_custom_network(name) {
    return Ok(None);
  }
  persist_network(requested_name, network, state)
    .await
    .map(Some)
}

async fn refresh_network_unlocked(
  docker_reference: &str,
  network_name: Option<&str>,
  state: &SystemState,
) -> IoResult<Option<String>> {
  if docker_reference.is_empty() {
    return Ok(None);
  }
  match inspect_network(docker_reference, state).await {
    Ok(network) => {
      let requested_name = network_name.unwrap_or(docker_reference);
      persist_inspected_network(requested_name, network, state).await
    }
    Err(err) if has_status(&err, 404) => {
      match missing_network_action(docker_reference, network_name) {
        MissingNetworkAction::ReinspectByName(network_name) => {
          match inspect_network(network_name, state).await {
            Ok(network) => {
              persist_inspected_network(network_name, network, state).await
            }
            Err(err) if has_status(&err, 404) => {
              remove_network_record(network_name, state).await?;
              Ok(None)
            }
            Err(err) => Err(docker_error(
              err,
              format!(
                "Unable to refresh Docker network {network_name} after stale reference {docker_reference}"
              ),
            )),
          }
        }
        MissingNetworkAction::RemoveRecord(network_name) => {
          remove_network_record(network_name, state).await?;
          Ok(None)
        }
        MissingNetworkAction::Ignore => Ok(None),
      }
    }
    Err(err) => Err(docker_error(
      err,
      format!("Unable to refresh Docker network {docker_reference}"),
    )),
  }
}

/// Refresh a persisted network by inspecting Docker only.
///
/// Unlike [`ensure_network_exists`], this lifecycle path never creates a
/// missing Docker network. A stale ID is retried by canonical name so a
/// same-name replacement is persisted; the row is removed only when that name
/// is also absent.
pub async fn refresh_network(
  docker_reference: &str,
  network_name: Option<&str>,
  state: &SystemState,
) -> IoResult<Option<String>> {
  let _guard = lifecycle_lock().lock().await;
  refresh_network_unlocked(docker_reference, network_name, state).await
}

fn stale_network_filter(
  node_name: &str,
  live_keys: &BTreeSet<String>,
) -> GenericFilter {
  let mut filter = GenericFilter::new()
    .r#where("node_name", GenericClause::Eq(node_name.to_owned()));
  if !live_keys.is_empty() {
    filter = filter.r#where(
      "key",
      GenericClause::NotIn(live_keys.iter().cloned().collect()),
    );
  }
  filter
}

/// Inventory every Docker user-defined network on the current node, refresh
/// its inspection snapshot, and prune rows which no longer exist locally.
///
/// Pruning only occurs after Docker's list call and every non-racy inspection
/// succeeds. Networks which disappear between list and inspect are treated as
/// stale; other Docker errors abort without pruning.
pub async fn reconcile_networks(state: &SystemState) -> IoResult<()> {
  let _guard = lifecycle_lock().lock().await;
  let filters = HashMap::from([("type".to_owned(), vec!["custom".to_owned()])]);
  let networks = state
    .inner
    .docker_api
    .list_networks(Some(ListNetworksOptions { filters }))
    .await
    .map_err(|err| {
      docker_error(err, "Unable to list Docker custom networks".to_owned())
    })?;

  let mut live_keys = BTreeSet::new();
  for network in networks {
    let network_name = network.name.as_deref().filter(|name| !name.is_empty());
    let docker_reference = network
      .id
      .as_deref()
      .filter(|id| !id.is_empty())
      .or(network_name);
    let Some(docker_reference) = docker_reference else {
      log::warn!(
        "network::reconcile: Docker returned a network without id/name"
      );
      continue;
    };
    if let Some(name) =
      refresh_network_unlocked(docker_reference, network_name, state).await?
    {
      live_keys.insert(utils::key::gen_network_key(
        &state.inner.config.hostname,
        &name,
      ));
    }
  }

  let filter = stale_network_filter(&state.inner.config.hostname, &live_keys);
  NetworkDb::del_by(&filter, &state.inner.pool).await
}

/// Ensure that a user-defined network exists on the current Docker node and
/// persist its latest inspection data.
///
/// Returns the inspected network's canonical name, or `None` when the mode is
/// a Docker built-in mode rather than a user-defined network.
pub async fn ensure_network_exists(
  network_mode: &str,
  state: &SystemState,
) -> IoResult<Option<String>> {
  if !is_custom_network(network_mode) {
    return Ok(None);
  }

  let _guard = lifecycle_lock().lock().await;
  ensure_network_exists_unlocked(network_mode, state)
    .await
    .map(Some)
}

async fn ensure_network_exists_unlocked(
  network_mode: &str,
  state: &SystemState,
) -> IoResult<String> {
  let network = match inspect_network(network_mode, state).await {
    Ok(network) => network,
    Err(err) if has_status(&err, 404) => {
      let labels = HashMap::from([
        ("io.nanocl".to_owned(), "enabled".to_owned()),
        ("io.nanocl.kind".to_owned(), "network".to_owned()),
      ]);
      let options = CreateNetworkOptions {
        name: network_mode.to_owned(),
        check_duplicate: true,
        driver: "bridge".to_owned(),
        internal: false,
        attachable: true,
        ingress: false,
        enable_ipv6: false,
        labels,
        ..Default::default()
      };
      match state.inner.docker_api.create_network(options).await {
        Ok(_) => {}
        // Another start task may have created the same network after our
        // initial inspection. Re-inspection below is the source of truth.
        Err(err) if has_status(&err, 409) => {}
        Err(err) => {
          return Err(docker_error(
            err,
            format!("Unable to create Docker network {network_mode}"),
          ));
        }
      }
      inspect_network(network_mode, state).await.map_err(|err| {
        docker_error(
          err,
          format!(
            "Unable to inspect Docker network {network_mode} after create"
          ),
        )
      })?
    }
    Err(err) => {
      return Err(docker_error(
        err,
        format!("Unable to inspect Docker network {network_mode}"),
      ));
    }
  };

  persist_network(network_mode, network, state).await
}

/// Ensure each effective mode once, in deterministic order.
pub async fn ensure_networks(
  network_modes: impl IntoIterator<Item = String>,
  state: &SystemState,
) -> IoResult<()> {
  let network_modes = custom_network_modes(network_modes);
  if network_modes.is_empty() {
    return Ok(());
  }
  let _guard = lifecycle_lock().lock().await;
  for network_mode in network_modes {
    let _ = ensure_network_exists_unlocked(&network_mode, state).await?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reserved_modes_are_not_custom_networks() {
    for mode in [
      "",
      "default",
      "bridge",
      "host",
      "none",
      "container:api",
      "container:0123456789",
    ] {
      assert!(!is_custom_network(mode), "{mode} must be reserved");
    }
  }

  #[test]
  fn named_modes_are_custom_networks() {
    for mode in [DEFAULT_NETWORK, "private-api", "containerized"] {
      assert!(is_custom_network(mode), "{mode} must be custom");
    }
  }

  #[test]
  fn only_missing_modes_use_nanocl_default() {
    assert_eq!(resolve_network_mode(None), DEFAULT_NETWORK);
    assert_eq!(resolve_network_mode(Some(String::new())), "");
    assert_eq!(resolve_network_mode(Some("host".into())), "host");
    assert_eq!(
      resolve_network_mode(Some("private-api".into())),
      "private-api"
    );
  }

  #[test]
  fn container_mode_uses_host_config_or_default() {
    let default_container = Config::default();
    assert_eq!(container_network_mode(&default_container), DEFAULT_NETWORK);

    let host_container = Config {
      host_config: Some(bollard_next::service::HostConfig {
        network_mode: Some("host".into()),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert_eq!(container_network_mode(&host_container), "host");
  }

  #[test]
  fn network_preflight_deduplicates_names_and_omits_reserved_modes() {
    assert_eq!(
      custom_network_modes([
        "private-b".to_owned(),
        "host".to_owned(),
        "private-a".to_owned(),
        "private-b".to_owned(),
        "container:api".to_owned(),
      ]),
      BTreeSet::from(["private-a".to_owned(), "private-b".to_owned()])
    );
  }

  #[test]
  fn stale_id_reinspects_the_canonical_network_name() {
    assert_eq!(
      missing_network_action("old-network-id", Some("private-api")),
      MissingNetworkAction::ReinspectByName("private-api")
    );
  }

  #[test]
  fn direct_missing_name_removes_only_its_own_record() {
    assert_eq!(
      missing_network_action("private-api", Some("private-api")),
      MissingNetworkAction::RemoveRecord("private-api")
    );
    assert_eq!(
      missing_network_action("old-network-id", None),
      MissingNetworkAction::Ignore
    );
    assert_eq!(
      missing_network_action("old-network-id", Some("bridge")),
      MissingNetworkAction::Ignore
    );
  }

  #[test]
  fn stale_filter_is_scoped_to_the_local_node() {
    let filter = stale_network_filter("node-a", &BTreeSet::new());
    let conditions = filter.r#where.unwrap().conditions;
    assert!(matches!(
      conditions.get("node_name"),
      Some(GenericClause::Eq(node)) if node == "node-a"
    ));
    assert!(!conditions.contains_key("key"));
  }

  #[test]
  fn stale_filter_preserves_every_live_key() {
    let live_keys = BTreeSet::from([
      "node-a.nanoclbr0".to_owned(),
      "node-a.private-api".to_owned(),
    ]);
    let filter = stale_network_filter("node-a", &live_keys);
    let conditions = filter.r#where.unwrap().conditions;
    assert!(matches!(
      conditions.get("key"),
      Some(GenericClause::NotIn(keys))
        if keys == &vec![
          "node-a.nanoclbr0".to_owned(),
          "node-a.private-api".to_owned(),
        ]
    ));
  }
}
