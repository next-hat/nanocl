use std::collections::HashMap;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use bollard_next::service::ContainerInspectResponse;
use bollard_next::container::{ListContainersOptions, InspectContainerOptions};

use nanocl_error::io::{FromIo, IoResult};

use nanocl_stubs::namespace::NamespacePartial;
use nanocl_stubs::cargo_spec::CargoSpecPartial;

use crate::{utils, version};
use crate::models::{
  DaemonState, ContainerPartial, ContainerUpdateDb, CargoDb, ContainerDb,
  Repository, Container, NamespaceDb,
};

/// Will determine if the instance is registered by nanocl
/// and sync his data with our store accordinly
pub(crate) async fn sync_instance(
  instance: &ContainerInspectResponse,
  state: &DaemonState,
) -> IoResult<()> {
  let name = instance.name.clone().unwrap_or_default().replace('/', "");
  let id = instance.id.clone().unwrap_or_default();
  let container_instance_data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "ContainerInstance"))?;
  let current_res = ContainerDb::find_by_pk(&id, &state.pool).await?;
  let labels = instance
    .config
    .clone()
    .unwrap_or_default()
    .labels
    .unwrap_or_default();
  let mut kind = "unknow";
  let mut kind_id = "";
  if let Some(job_name) = labels.get("io.nanocl.j") {
    kind = "Job";
    kind_id = job_name;
  }
  if let Some(cargo_key) = labels.get("io.nanocl.c") {
    kind = "Cargo";
    kind_id = cargo_key;
  }
  if let Some(vm_key) = labels.get("io.nanocl.v") {
    kind = "Vm";
    kind_id = vm_key;
  }
  if kind == "unknow" {
    return Ok(());
  }
  match current_res {
    Ok(current_instance) => {
      let current_instance: Container = current_instance.try_into()?;
      if current_instance.data == *instance {
        log::debug!("container instance already synced");
        return Ok(());
      }
      let new_instance = ContainerUpdateDb {
        updated_at: Some(chrono::Utc::now().naive_utc()),
        data: Some(container_instance_data),
      };
      let _ = ContainerDb::update_by_pk(&id, new_instance, &state.pool)
        .await
        .map_err(|err| log::error!("{err}"));
    }
    Err(_) => {
      let new_instance = ContainerPartial {
        key: id,
        name,
        kind: kind.to_owned(),
        data: container_instance_data.clone(),
        node_id: state.config.hostname.clone(),
        kind_id: kind_id.to_owned(),
      };
      let _ = ContainerDb::create(&new_instance, &state.pool)
        .await
        .map_err(|err| log::error!("{err}"));
    }
  }
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
  state: &DaemonState,
) -> IoResult<()> {
  if NamespaceDb::find_by_pk(name, &state.pool).await?.is_ok() {
    return Ok(());
  }
  let new_nsp = NamespacePartial {
    name: name.to_owned(),
  };
  if create_network {
    utils::namespace::create(&new_nsp, state).await?;
  } else {
    NamespaceDb::create(&new_nsp, &state.pool).await??;
  }
  Ok(())
}

/// Convert existing container instances with our labels to cargo.
/// We use it to be sure that all existing containers are registered as cargo.
pub(crate) async fn sync_instances(state: &DaemonState) -> IoResult<()> {
  log::info!("Syncing existing container");
  let options = Some(ListContainersOptions::<&str> {
    all: true,
    ..Default::default()
  });
  let containers = state
    .docker_api
    .list_containers(options)
    .await
    .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
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
    let container = state
      .docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
    let config = container.config.clone().unwrap_or_default();
    let mut config: bollard_next::container::Config = config.into();
    config.host_config = container.host_config.clone();
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
    let new_cargo = CargoSpecPartial {
      name: metadata[0].to_owned(),
      container: config.to_owned(),
      ..Default::default()
    };
    cargo_inspected.insert(metadata[0].to_owned(), true);
    match CargoDb::inspect_by_pk(cargo_key, &state.pool).await {
      // If the cargo is already in our store and the config is different we update it
      Ok(cargo) => {
        if cargo.spec.container != config {
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[0],
            metadata[1]
          );
          CargoDb::update_from_spec(
            cargo_key,
            &new_cargo,
            &format!("v{}", version::VERSION),
            &state.pool,
          )
          .await?;
          sync_instance(&container, state).await?;
        }
      }
      // unless we create his config
      Err(_err) => {
        log::debug!(
          "creating cargo {} in namespace {}",
          metadata[0],
          metadata[1]
        );
        if NamespaceDb::find_by_pk(metadata[1], &state.pool)
          .await
          .is_err()
        {
          NamespaceDb::create(
            &NamespacePartial {
              name: metadata[1].to_owned(),
            },
            &state.pool,
          )
          .await??;
        }
        CargoDb::create_from_spec(
          metadata[1],
          &new_cargo,
          &format!("v{}", version::VERSION),
          &state.pool,
        )
        .await?;
        sync_instance(&container, state).await?;
      }
    }
  }
  log::info!("Container synced");
  Ok(())
}

/// Check for vm images inside the vm images directory
/// and create them in the database if they don't exist
pub(crate) async fn sync_vm_images(state: &DaemonState) -> IoResult<()> {
  log::info!("Syncing existing VM images");
  let files =
    std::fs::read_dir(format!("{}/vms/images", &state.config.state_dir))?;
  files
    .into_iter()
    .map(|file| async {
      let file = file?;
      let file_name = file.file_name();
      let file_name = file_name.to_str().unwrap_or_default();
      let dot_split_name = file_name.split('.').collect::<Vec<&str>>();
      let name = if dot_split_name.len() > 1 {
        dot_split_name[..dot_split_name.len() - 1].join(".")
      } else {
        dot_split_name[0].to_owned()
      };
      let file_path = file.path();
      let path = file_path.to_str().unwrap_or_default();
      if let Err(error) =
        utils::vm_image::create(&name, path, &state.pool).await
      {
        log::warn!("{error}")
      }
      Ok::<_, std::io::Error>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<Result<_, std::io::Error>>>()
    .await;
  log::info!("Synced VM images");
  Ok(())
}
