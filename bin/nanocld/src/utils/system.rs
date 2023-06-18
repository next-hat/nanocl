use std::collections::HashMap;

use bollard_next::container::{ListContainersOptions, InspectContainerOptions};

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use nanocl_stubs::config::DaemonConfig;
use nanocl_utils::io_error::{FromIo, IoResult};

use nanocl_stubs::namespace::NamespacePartial;
use nanocl_stubs::cargo_config::CargoConfigPartial;

use crate::version::VERSION;
use crate::{utils, repositories};
use crate::models::{Pool, DaemonState};

/// Ensure existance of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace used by internal nanocl components.
/// where global is the namespace used by default.
/// User can registed they own namespace to ensure better encaptusation.
pub async fn register_namespace(
  name: &str,
  create_network: bool,
  state: &DaemonState,
) -> IoResult<()> {
  if repositories::namespace::exist_by_name(name, &state.pool).await? {
    return Ok(());
  }
  let new_nsp = NamespacePartial {
    name: name.to_owned(),
  };
  if create_network {
    utils::namespace::create(&new_nsp, state).await?;
  } else {
    repositories::namespace::create(&new_nsp, &state.pool).await?;
  }
  Ok(())
}

/// Convert existing containers with our labels to cargo.
/// We use it to be sure that all existing containers are registered as cargo.
pub async fn sync_containers(
  docker_api: &bollard_next::Docker,
  pool: &Pool,
) -> IoResult<()> {
  log::info!("Syncing existing container");
  let options = Some(ListContainersOptions::<&str> {
    all: true,
    ..Default::default()
  });
  let containers = docker_api
    .list_containers(options)
    .await
    .map_err(|err| err.map_err_context(|| "ListContainer"))?;
  let mut cargo_inspected: HashMap<String, bool> = HashMap::new();
  for container_summary in containers {
    // extract cargo name and namespace
    let labels = container_summary.labels.unwrap_or_default();
    let Some(cargo_key) = labels.get("io.nanocl.c") else {
      // We don't have cargo label, we skip it
      continue;
    };
    let metadata = cargo_key.split('.').collect::<Vec<&str>>();
    if metadata.len() < 2 {
      // We don't have cargo label well formated, we skip it
      continue;
    }
    // If we already inspected this cargo we skip it
    if cargo_inspected.contains_key(metadata[0]) {
      continue;
    }

    // We inspect the container to have all the information we need
    let container = docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
    let config = container.config.unwrap_or_default();
    let mut config: bollard_next::container::Config = config.into();
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
      name: metadata[0].to_owned(),
      container: config.to_owned(),
      ..Default::default()
    };

    cargo_inspected.insert(metadata[0].to_owned(), true);
    match repositories::cargo::inspect_by_key(cargo_key, pool).await {
      // If the cargo is already in our store and the config is different we update it
      Ok(cargo) => {
        if cargo.config.container != config {
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[0],
            metadata[1]
          );
          repositories::cargo::update_by_key(
            cargo_key,
            &new_cargo,
            &format!("v{}", VERSION),
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
          metadata[0],
          metadata[1]
        );

        if repositories::namespace::find_by_name(metadata[1], pool)
          .await
          .is_err()
        {
          repositories::namespace::create(
            &NamespacePartial {
              name: metadata[1].to_owned(),
            },
            pool,
          )
          .await?;
        }

        repositories::cargo::create(
          metadata[1],
          &new_cargo,
          &format!("v{}", VERSION),
          pool,
        )
        .await?;
      }
    }
  }

  log::info!("Container synced");
  Ok(())
}

pub async fn sync_vm_images(
  daemon_conf: &DaemonConfig,
  pool: &Pool,
) -> IoResult<()> {
  log::info!("Syncing existing vm");
  let files =
    std::fs::read_dir(format!("{}/vms/images", &daemon_conf.state_dir))?;

  files
    .into_iter()
    .map(|file| async {
      let file = file?;
      let file_name = file.file_name();
      let name = file_name.to_str().unwrap_or_default();

      let file_path = file.path();
      let path = file_path.to_str().unwrap_or_default();

      if let Err(error) = utils::vm_image::create(name, path, pool).await {
        log::warn!("{error}")
      }
      Ok::<_, std::io::Error>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<_, std::io::Error>>>()
    .await;
  log::info!("VM Image synced");
  Ok(())
}
