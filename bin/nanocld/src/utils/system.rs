use std::collections::HashMap;

use ntex::rt;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use bollard_next::system::EventsOptions;
use bollard_next::service::{
  EventMessageTypeEnum, ContainerInspectResponse, EventMessage,
};
use bollard_next::container::{ListContainersOptions, InspectContainerOptions};

use nanocl_error::io::{FromIo, IoResult};

use nanocl_stubs::namespace::NamespacePartial;
use nanocl_stubs::cargo_spec::CargoSpecPartial;

use crate::{version, utils, repositories};
use crate::models::{
  DaemonState, ContainerInstancePartial, ContainerInstanceUpdateDb,
};

/// Sync instance
///
/// Will determine if the instance is registered by nanocl
/// and sync his data with our store accordinly
///
///
/// ## Arguments
///
/// * [instance](ContainerInspectResponse) - The instance to sync
/// * [state](DaemonState) - The state
///
async fn sync_instance(
  instance: &ContainerInspectResponse,
  state: &DaemonState,
) -> IoResult<()> {
  let name = instance.name.clone().unwrap_or_default().replace('/', "");
  let id = instance.id.clone().unwrap_or_default();
  let container_instance_data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "ContainerInstance"))?;
  let current_res =
    repositories::container_instance::find_by_id(&id, &state.pool).await;
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
      if current_instance.data == *instance {
        log::debug!("container instance already synced");
        return Ok(());
      }
      let new_instance = ContainerInstanceUpdateDb {
        updated_at: Some(chrono::Utc::now().naive_utc()),
        data: Some(container_instance_data),
      };
      let _ = repositories::container_instance::update(
        &id,
        &new_instance,
        &state.pool,
      )
      .await
      .map_err(|err| log::error!("{err}"));
    }
    Err(_) => {
      let new_instance = ContainerInstancePartial {
        key: id,
        name,
        kind: kind.to_owned(),
        data: container_instance_data.clone(),
        node_id: state.config.hostname.clone(),
        kind_id: kind_id.to_owned(),
      };
      let _ =
        repositories::container_instance::create(&new_instance, &state.pool)
          .await
          .map_err(|err| log::error!("{err}"));
    }
  }
  Ok(())
}

/// ## Exec docker event
///
/// Take corresponding action depending on the docker event
/// eg: update/create/destroy a container instance
///
/// ## Arguments
///
/// * [event](EventMessage) - The docker event
/// * [state](DaemonState) - The state
///
async fn exec_docker_event(
  event: &EventMessage,
  state: &DaemonState,
) -> IoResult<()> {
  let kind = event.typ.unwrap_or(EventMessageTypeEnum::EMPTY);
  if kind != EventMessageTypeEnum::CONTAINER {
    return Ok(());
  }
  let actor = event.actor.clone().unwrap_or_default();
  let attributes = actor.attributes.unwrap_or_default();
  if attributes.get("io.nanocl").is_none() {
    return Ok(());
  }
  let action = event.action.clone().unwrap_or_default();
  let id = actor.id.unwrap_or_default();
  log::debug!("docker event: {action}");
  if action.as_str() == "destroy" {
    log::debug!("docker event destroy container: {id}");
    repositories::container_instance::delete_by_id(&id, &state.pool).await?;
    return Ok(());
  }
  let instance = state
    .docker_api
    .inspect_container(&id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  sync_instance(&instance, state).await?;
  Ok(())
}

/// ## Register namespace
///
/// Ensure existance of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace used by internal nanocl components.
/// where global is the namespace used by default.
/// User can registed they own namespace to ensure better encaptusation.
///
/// ## Arguments
///
/// * [name](str) Name of the namespace to register
/// * [create_network](bool) If true we create the network for the namespace
/// * [state](DaemonState) The daemon state
///
pub(crate) async fn register_namespace(
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

/// ## Sync instances
///
/// Convert existing container instances with our labels to cargo.
/// We use it to be sure that all existing containers are registered as cargo.
///
/// ## Arguments
///
/// * [state](DaemonState) - The state
///
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
    let container = state
      .docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
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
    match repositories::cargo::inspect_by_key(cargo_key, &state.pool).await {
      // If the cargo is already in our store and the config is different we update it
      Ok(cargo) => {
        if cargo.spec.container != config {
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[0],
            metadata[1]
          );
          repositories::cargo::update_by_key(
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
        if repositories::namespace::find_by_name(metadata[1], &state.pool)
          .await
          .is_err()
        {
          repositories::namespace::create(
            &NamespacePartial {
              name: metadata[1].to_owned(),
            },
            &state.pool,
          )
          .await?;
        }
        repositories::cargo::create(
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

/// ## Sync vm images
///
/// Check for vm images inside the vm images directory
/// and create them in the database if they don't exist
///
/// ## Arguments
///
/// * [state](DaemonState) - The state
///
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

/// ## Watch docker events
///
/// Create a new thread with his own loop to watch docker events and update
/// container instance accordingly
///
pub(crate) fn watch_docker(state: &DaemonState) {
  let state = state.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      loop {
        let mut streams =
          state.docker_api.events(None::<EventsOptions<String>>);
        while let Some(event) = streams.next().await {
          match event {
            Ok(event) => {
              if let Err(err) = exec_docker_event(&event, &state).await {
                log::warn!("docker event error: {err:?}")
              }
            }
            Err(err) => {
              log::warn!("docker event error: {:?}", err);
            }
          }
        }
        log::warn!("disconnected from docker trying to reconnect");
        ntex::time::sleep(std::time::Duration::from_secs(1)).await;
      }
    });
  });
}
