use std::collections::HashMap;

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
  vars, utils,
  repositories::generic::*,
  models::{
    SystemState, CargoDb, ProcessDb, NamespaceDb, VmImageDb, ProcessUpdateDb,
    CargoObjCreateIn,
  },
  objects::generic::ObjCreate,
};

/// Will determine if the instance is registered by nanocl
/// and sync his data with our store accordingly
pub async fn sync_process(
  key: &str,
  kind: &str,
  instance: &ContainerInspectResponse,
  state: &SystemState,
) -> IoResult<()> {
  let id = instance.id.clone().unwrap_or_default();
  let created_at = instance.created.clone().unwrap_or_default();
  let name = instance.name.clone().unwrap_or_default().replace('/', "");
  log::trace!("system::sync_process: {name}");
  let container_instance_data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Process"))?;
  let current_res = ProcessDb::transform_read_by_pk(&id, &state.pool).await;
  match current_res {
    Ok(current_instance) => {
      if current_instance.data == *instance {
        log::info!("system::sync_process: {name} is up to date");
        return Ok(());
      }
      let new_instance = ProcessUpdateDb {
        name: Some(name.to_owned()),
        updated_at: Some(chrono::Utc::now().naive_utc()),
        data: Some(container_instance_data),
        ..Default::default()
      };
      ProcessDb::update_pk(&current_instance.key, new_instance, &state.pool)
        .await?;
      log::info!("system::sync_process: {name} updated");
    }
    Err(_) => {
      let new_instance = ProcessPartial {
        key: id,
        name: name.clone(),
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
      ProcessDb::create_from(&new_instance, &state.pool).await?;
      log::info!("system::sync_process: {name} created");
    }
  }
  Ok(())
}

/// Ensure existance of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace used by internal nanocl components.
/// where global is the namespace used by default.
/// User can registed they own namespace to ensure better encaptusation.
pub async fn register_namespace(
  name: &str,
  create_network: bool,
  state: &SystemState,
) -> IoResult<()> {
  if NamespaceDb::read_by_pk(name, &state.pool).await.is_ok() {
    return Ok(());
  }
  let new_nsp = NamespacePartial {
    name: name.to_owned(),
  };
  if create_network {
    NamespaceDb::create_obj(&new_nsp, state).await?;
  } else {
    NamespaceDb::create_from(&new_nsp, &state.pool).await?;
  }
  Ok(())
}

/// Convert existing container instances with our labels to cargo.
/// We use it to be sure that all existing containers are registered as cargo.
pub async fn sync_processes(state: &SystemState) -> IoResult<()> {
  log::info!("system::sync_processes: starting");
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
  let mut ids = Vec::new();
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
    let id = container_summary.id.unwrap_or_default();
    let container = state
      .docker_api
      .inspect_container(&id, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
    sync_process(key, kind, &container, state).await?;
    ids.push(id);
    if kind == "cargo" {
      let metadata = key.split('.').collect::<Vec<&str>>();
      let [name, namespace] = metadata[..] else {
        continue;
      };
      // We inspect the container to have all the information we need
      // If we already inspected this cargo we skip it
      if cargo_inspected.contains_key(key) {
        continue;
      }
      let config = container.config.clone().unwrap_or_default();
      let mut config: bollard_next::container::Config = config.into();
      config.host_config = container.host_config.clone();
      let new_cargo = CargoSpecPartial {
        name: name.to_owned(),
        container: config.to_owned(),
        ..Default::default()
      };
      cargo_inspected.insert(key.to_owned(), true);
      match CargoDb::transform_read_by_pk(key, &state.pool).await {
        // unless we create his config
        Err(_err) => {
          if let Err(err) = register_namespace(namespace, false, state).await {
            log::warn!("system::sync_processes: namespace {err}");
            continue;
          }
          log::trace!(
            "system::sync_processes: create cargo {name} in namespace {namespace}",
          );
          let obj = &CargoObjCreateIn {
            namespace: namespace.to_owned(),
            spec: new_cargo.clone(),
            version: format!("v{}", vars::VERSION),
          };
          CargoDb::create_obj(obj, state).await?;
        }
        // If the cargo is already in our store and the config is different we update it
        Ok(cargo) => {
          if cargo.spec.container == config {
            continue;
          }
          log::trace!(
            "system::sync_processes: update cargo {name} in namespace {namespace}",
          );
          CargoDb::update_from_spec(
            key,
            &new_cargo,
            &format!("v{}", vars::VERSION),
            &state.pool,
          )
          .await?;
        }
      }
    }
  }
  // delete zombie instances (not in docker anymore) from our store if any
  let filter = GenericFilter::new()
    .r#where(
      "key",
      GenericClause::NotIn(ids.iter().map(|id| id.to_owned()).collect()),
    )
    .r#where("node_key", GenericClause::Eq(state.config.hostname.clone()));
  ProcessDb::del_by(&filter, &state.pool).await?;
  log::info!("system::sync_processes: done");
  Ok(())
}

/// Check for vm images inside the vm images directory
/// and create them in the database if they don't exist
pub async fn sync_vm_images(state: &SystemState) -> IoResult<()> {
  log::info!("system::sync_vm_images: start");
  let files =
    std::fs::read_dir(format!("{}/vms/images", &state.config.state_dir))?;
  for file in files {
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
    if VmImageDb::read_by_pk(&name, &state.pool).await.is_ok() {
      continue;
    }
    if let Err(error) = utils::vm_image::create(&name, path, &state.pool).await
    {
      log::warn!("system::sync_vm_images: {error}")
    }
  }
  log::info!("system::sync_vm_images: done");
  Ok(())
}
