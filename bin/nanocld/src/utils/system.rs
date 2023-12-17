use std::collections::HashMap;

use diesel::prelude::*;
use futures_util::{StreamExt, stream::FuturesUnordered};

use nanocl_error::io::{FromIo, IoResult, IoError};

use bollard_next::{
  service::ContainerInspectResponse,
  container::{ListContainersOptions, InspectContainerOptions},
};
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  namespace::NamespacePartial,
  cargo_spec::CargoSpecPartial,
  process::ProcessPartial,
};

use crate::{
  utils, version,
  models::{
    DaemonState, ProcessUpdateDb, CargoDb, ProcessDb, Repository, NamespaceDb,
    VmImageDb,
  },
};

/// Will determine if the instance is registered by nanocl
/// and sync his data with our store accordinly
pub(crate) async fn sync_instance(
  key: &str,
  kind: &str,
  instance: &ContainerInspectResponse,
  state: &DaemonState,
) -> IoResult<()> {
  let id = instance.id.clone().unwrap_or_default();
  let created_at = instance.created.clone().unwrap_or_default();
  let name = instance.name.clone().unwrap_or_default().replace('/', "");
  log::debug!("syncing instance: {id}:{name} created_at: {created_at}");
  let container_instance_data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Process"))?;
  let filter =
    GenericFilter::new().r#where("key", GenericClause::Eq(id.to_owned()));
  let current_res = ProcessDb::find_one(&filter, &state.pool).await?;
  match current_res {
    Ok(current_instance) => {
      if current_instance.data == *instance {
        log::debug!("container instance already synced");
        return Ok(());
      }
      let new_instance = ProcessUpdateDb {
        name: Some(name.to_owned()),
        updated_at: Some(chrono::Utc::now().naive_utc()),
        data: Some(container_instance_data),
        ..Default::default()
      };
      ProcessDb::update_by_pk(&current_instance.key, new_instance, &state.pool)
        .await??;
    }
    Err(_) => {
      let new_instance = ProcessPartial {
        key: id,
        name,
        kind: kind.to_owned().try_into()?,
        data: container_instance_data.clone(),
        node_key: state.config.hostname.clone(),
        kind_key: key.to_owned(),
        created_at: Some(
          chrono::NaiveDateTime::parse_from_str(
            &created_at,
            "%Y-%m-%dT%H:%M:%S%.fZ",
          )
          .map_err(|err| {
            IoError::invalid_data("ProcessDb", &err.to_string())
          })?,
        ),
      };
      ProcessDb::create(&new_instance, &state.pool).await??;
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
  ProcessDb::delete_by(
    crate::schema::processes::columns::node_key
      .eq(state.config.hostname.clone()),
    &state.pool,
  )
  .await??;
  for container_summary in containers {
    let labels = container_summary.labels.unwrap_or_default();
    let Some(kind) = labels.get("io.nanocl.kind") else {
      continue;
    };
    let key = match kind.as_str() {
      "cargo" => labels.get("io.nanocl.c"),
      "job" => labels.get("io.nanocl.j"),
      "vm" => labels.get("io.nanocl.v"),
      _ => continue,
    };
    let Some(key) = key else {
      continue;
    };
    let container = state
      .docker_api
      .inspect_container(
        &container_summary.id.unwrap_or_default(),
        None::<InspectContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
    sync_instance(key, kind, &container, state).await?;
    if kind == "cargo" {
      let metadata = key.split('.').collect::<Vec<&str>>();
      if metadata.len() < 2 {
        // We don't have cargo label well formated, we skip it
        continue;
      }
      // We inspect the container to have all the information we need
      // If we already inspected this cargo we skip it
      if cargo_inspected.contains_key(key) {
        continue;
      }
      let config = container.config.clone().unwrap_or_default();
      let mut config: bollard_next::container::Config = config.into();
      config.host_config = container.host_config.clone();
      let new_cargo = CargoSpecPartial {
        name: metadata[0].to_owned(),
        container: config.to_owned(),
        ..Default::default()
      };
      cargo_inspected.insert(key.to_owned(), true);
      match CargoDb::inspect_by_pk(key, &state.pool).await {
        // If the cargo is already in our store and the config is different we update it
        Ok(cargo) => {
          if cargo.spec.container == config {
            continue;
          }
          log::debug!(
            "updating cargo {} in namespace {}",
            metadata[0],
            metadata[1]
          );
          CargoDb::update_from_spec(
            key,
            &new_cargo,
            &format!("v{}", version::VERSION),
            &state.pool,
          )
          .await?;
        }
        // unless we create his config
        Err(_err) => {
          log::debug!(
            "syncing cargo {} in namespace {}",
            metadata[0],
            metadata[1]
          );
          if let Err(err) = register_namespace(metadata[1], false, state).await
          {
            log::warn!("error while syncing namespace: {err:?}");
            continue;
          }
          CargoDb::create_from_spec(
            metadata[1],
            &new_cargo,
            &format!("v{}", version::VERSION),
            &state.pool,
          )
          .await?;
        }
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
      if VmImageDb::find_by_pk(&name, &state.pool).await.is_ok() {
        return Ok::<_, std::io::Error>(());
      }
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
