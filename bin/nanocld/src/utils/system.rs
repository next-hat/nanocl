use std::collections::HashMap;

use nanocl_error::io::{FromIo, IoError, IoResult};

use bollard_next::{
  container::{InspectContainerOptions, ListContainersOptions},
  service::ContainerInspectResponse,
};
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  namespace::NamespacePartial,
  process::ProcessPartial,
};

use crate::{
  models::{
    CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
    NamespaceDb, NewCargoReplicaProcessDb, ProcessDb, ProcessUpdateDb,
    SystemState,
  },
  repositories::generic::*,
  utils::container::cargo_compiler::{
    LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA, LABEL_REPLICA_ORDINAL,
    LABEL_ROLE,
  },
};

async fn reattach_cargo_process(
  cargo_key: &str,
  process_key: &str,
  process_name: &str,
  labels: &HashMap<String, String>,
  state: &SystemState,
) -> IoResult<()> {
  let Some(replica_key) = labels.get(LABEL_REPLICA) else {
    return Ok(());
  };
  let replica_key = replica_key.parse::<uuid::Uuid>().map_err(|error| {
    IoError::other(
      "CargoRuntimeIdentity",
      &format!("Cargo process {process_key} has invalid replica UUID: {error}"),
    )
  })?;
  let ordinal = labels
    .get(LABEL_REPLICA_ORDINAL)
    .ok_or_else(|| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!("Cargo process {process_key} has no replica ordinal"),
      )
    })?
    .parse::<i32>()
    .map_err(|error| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo process {process_key} has invalid replica ordinal: {error}"
        ),
      )
    })?;
  let role = labels
    .get(LABEL_ROLE)
    .ok_or_else(|| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!("Cargo process {process_key} has no Cargo role"),
      )
    })?
    .parse::<CargoReplicaProcessRole>()?;
  let container_name = labels.get(LABEL_CONTAINER).ok_or_else(|| {
    IoError::other(
      "CargoRuntimeIdentity",
      &format!("Cargo process {process_key} has no logical container name"),
    )
  })?;
  let essential = labels
    .get(LABEL_ESSENTIAL)
    .ok_or_else(|| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!("Cargo process {process_key} has no essential marker"),
      )
    })?
    .parse::<bool>()
    .map_err(|error| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo process {process_key} has invalid essential marker: {error}"
        ),
      )
    })?;
  let replica = CargoReplicaDb::get(replica_key, &state.inner.pool).await?;
  if replica.cargo_key != cargo_key
    || replica.ordinal != ordinal
    || replica.node_name.as_deref() != Some(&state.inner.config.hostname)
  {
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo process {process_key} does not match replica {replica_key} assignment"
      ),
    ));
  }
  let mapping = match CargoReplicaProcessDb::find(
    replica_key,
    role,
    container_name,
    &state.inner.pool,
  )
  .await
  {
    Ok(mapping) => mapping,
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      CargoReplicaProcessDb::create(
        NewCargoReplicaProcessDb::new(
          replica_key,
          container_name,
          role,
          essential,
        ),
        &state.inner.pool,
      )
      .await?
    }
    Err(error) => return Err(error),
  };
  if mapping
    .process_key
    .as_deref()
    .is_some_and(|attached| attached != process_key)
  {
    if process_name.starts_with("tmp-")
      || process_name.starts_with("candidate-")
    {
      // A retained or pending rollout process intentionally shares the stable
      // logical identity with the attached generation. It is observed but
      // must not steal the durable attachment during startup inventory.
      return Ok(());
    }
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo mapping {} is already attached to process {:?}, not observed process {process_key}",
        mapping.key, mapping.process_key
      ),
    ));
  }
  CargoReplicaProcessDb::set_processes(
    vec![(mapping.key, Some(process_key.to_owned()), essential)],
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

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
  let current_res =
    ProcessDb::transform_read_by_pk(&id, &state.inner.pool).await;
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
      ProcessDb::update_pk(
        &current_instance.key,
        new_instance,
        &state.inner.pool,
      )
      .await?;
      log::info!("system::sync_process: {name} updated");
    }
    Err(_) => {
      let new_instance = ProcessPartial {
        key: id,
        name: name.clone(),
        kind: kind.to_owned().try_into()?,
        data: container_instance_data.clone(),
        node_name: state.inner.config.hostname.clone(),
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
      ProcessDb::create_from(&new_instance, &state.inner.pool).await?;
      log::info!("system::sync_process: {name} created");
    }
  }
  Ok(())
}

/// Ensure existence of specific namespace in our store.
/// We use it to be sure `system` and `global` namespace exists.
/// system is the namespace used by internal nanocl components.
/// where global is the namespace used by default.
/// User can registered they own namespace to ensure better encapsulation.
pub async fn register_namespace(
  name: &str,
  state: &SystemState,
) -> IoResult<()> {
  if NamespaceDb::read_by_pk(name, &state.inner.pool)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let new_nsp = NamespacePartial {
    name: name.to_owned(),
    metadata: None,
  };
  NamespaceDb::create_from(&new_nsp, &state.inner.pool).await?;
  Ok(())
}

/// Rebuild observed process rows from managed Docker containers.
///
/// Desired Cargo declarations remain database-authoritative: an effective
/// Docker inspect configuration cannot be reversed into the authored
/// multi-container declaration.
pub async fn sync_processes(state: &SystemState) -> IoResult<()> {
  log::info!("system::sync_processes: starting");
  let options = Some(ListContainersOptions::<&str> {
    all: true,
    ..Default::default()
  });
  let containers = state
    .inner
    .docker_api
    .list_containers(options)
    .await
    .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
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
      .inner
      .docker_api
      .inspect_container(&id, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "SyncInstance"))?;
    sync_process(key, kind, &container, state).await?;
    if kind == "cargo" {
      let process_name = container
        .name
        .as_deref()
        .unwrap_or_default()
        .trim_start_matches('/');
      reattach_cargo_process(key, &id, process_name, &labels, state).await?;
    }
    ids.push(id);
  }
  // delete zombie instances (not in docker anymore) from our store if any
  let filter = GenericFilter::new()
    .r#where(
      "key",
      GenericClause::NotIn(ids.iter().map(|id| id.to_owned()).collect()),
    )
    .r#where(
      "node_name",
      GenericClause::Eq(state.inner.config.hostname.clone()),
    );
  ProcessDb::del_by(&filter, &state.inner.pool).await?;
  log::info!("system::sync_processes: done");
  Ok(())
}
