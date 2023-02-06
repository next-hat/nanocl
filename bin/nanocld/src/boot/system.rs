use std::collections::HashMap;

use bollard::network::{CreateNetworkOptions, InspectNetworkOptions};
use bollard::container::{ListContainersOptions, InspectContainerOptions};

use nanocl_stubs::namespace::NamespacePartial;
use nanocl_stubs::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::error::DaemonError;
use crate::models::Pool;

/// Ensure existance of the system network that controllers will use.
/// It's ensure existance of a network in your system called `nanocl.system`
/// Also registered inside docker as `system` since it's the name of the namespace.
/// This network is created to be sure a store is running inside.
pub(crate) async fn ensure_network(
  name: &str,
  docker_api: &bollard::Docker,
) -> Result<(), DaemonError> {
  // Ensure network existance
  if docker_api
    .inspect_network(name, None::<InspectNetworkOptions<&str>>)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let mut options: HashMap<String, String> = HashMap::new();
  options.insert(
    String::from("com.docker.network.bridge.name"),
    format!("nanocl.{name}"),
  );
  let config = CreateNetworkOptions {
    name: name.to_owned(),
    driver: String::from("bridge"),
    options,
    ..Default::default()
  };
  docker_api.create_network(config).await?;
  Ok(())
}

/// Ensure existance of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace used by internal nanocl components.
/// where global is the namespace used by default.
/// User can registed they own namespace to ensure better encaptusation.
pub(crate) async fn register_namespace(
  name: &str,
  create_network: bool,
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<(), DaemonError> {
  if repositories::namespace::exist_by_name(name.to_owned(), pool).await? {
    return Ok(());
  }
  let new_nsp = NamespacePartial {
    name: name.to_owned(),
  };
  if create_network {
    utils::namespace::create(&new_nsp, docker_api, pool).await?;
  } else {
    repositories::namespace::create(new_nsp, pool).await?;
  }
  Ok(())
}

/// Convert existing containers with our labels to cargo.
/// We use it to be sure that all existing containers are registered as cargo.
pub(crate) async fn sync_containers(
  docker_api: &bollard::Docker,
  pool: &Pool,
) -> Result<(), DaemonError> {
  log::info!("Syncing existing container");
  let options = Some(ListContainersOptions::<&str> {
    all: true,
    ..Default::default()
  });
  let containers = docker_api.list_containers(options).await?;
  let mut cargo_inspected: HashMap<String, bool> = HashMap::new();
  for container_summary in containers {
    // extract cargo name and namespace
    let labels = container_summary.labels.unwrap_or_default();
    let Some(cargo_key) = labels.get("io.nanocl.cargo") else {
      // We don't have cargo label, we skip it
      continue;
    };
    let metadata = cargo_key.split('-').collect::<Vec<&str>>();
    if metadata.len() < 2 {
      // We don't have cargo label well formated, we skip it
      continue;
    }
    // If we already inspected this cargo we skip it
    if cargo_inspected.contains_key(metadata[1]) {
      continue;
    }

    // We inspect the container to have all the information we need
    let container = docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await?;
    let config = container.config.unwrap_or_default();
    let mut config: bollard::container::Config<String> = config.into();
    config.host_config = container.host_config;

    // TODO: handle network config
    // If the container is replicated by nanocl we should not have any network settings
    // Because we want docker to automatically set ip address and other network settings.
    // But if the container is not replicated by nanocl we may want to keep the network settings
    // Since container are automaticatly created or deleted by nanocl
    // We should not save any network settings because we want docker to automatically set ip address
    // and other network settings.
    // let network_settings = container.network_settings.unwrap_or_default();
    // if let Some(_endpoints_config) = network_settings.networks {
    //   // config.networking_config = Some(NetworkingConfig { endpoints_config });
    // }

    let new_cargo = CargoConfigPartial {
      name: metadata[1..].join("-"),
      container: config.to_owned(),
      ..Default::default()
    };

    cargo_inspected.insert(metadata[1].to_owned(), true);
    match repositories::cargo::inspect_by_key(cargo_key.to_owned(), pool).await
    {
      // If the cargo is already in our store and the config is different we update it
      Ok(cargo) => {
        if cargo.config.container != config {
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[1],
            metadata[0]
          );
          repositories::cargo::update_by_key(
            cargo_key.to_owned(),
            new_cargo,
            pool,
          )
          .await?;
        }
        continue;
      }
      // unless we create his config
      Err(_err) => {
        log::debug!(
          "creating cargo {} in namespace {}",
          metadata[1],
          metadata[0]
        );
        repositories::cargo::create(metadata[0].to_owned(), new_cargo, pool)
          .await?;
      }
    }
  }

  log::info!("Container synced");
  Ok(())
}
