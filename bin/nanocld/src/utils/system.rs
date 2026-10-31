mod cargo_bootstrap;

use std::{
  collections::{BTreeMap, HashMap, HashSet},
  sync::Arc,
};

use diesel::prelude::*;

use nanocl_error::io::{FromIo, IoError, IoResult};

use bollard_next::{
  models::ContainerInspectResponse,
  query_parameters::{InspectContainerOptions, ListContainersOptions},
};
use nanocl_stubs::{
  cargo::Cargo,
  cargo_spec::CARGO_CONTAINER_POSITION_LABEL,
  generic::{GenericClause, GenericFilter},
  namespace::NamespacePartial,
  process::ProcessPartial,
  resource_key::ResourceKey,
  system::{ObjPsHealthStatusKind, ObjPsStatusKind, ObjPsStatusPartial},
};

use crate::{
  models::{
    CargoDb, CargoReplicaDb, CargoReplicaProcessDb, CargoReplicaProcessRole,
    NamespaceDb, NewCargoReplicaDb, NewCargoReplicaProcessDb, ObjPsStatusDb,
    OperationDb, ProcessDb, ProcessUpdateDb, SpecDb, SystemState,
  },
  repositories::{
    cargo_operation::lock_bootstrap_cargo,
    generic::*,
    operation::{TransactionResult, run_query_owned, transaction},
  },
  schema::{
    cargo_replica_processes, cargo_replicas, cargoes, object_process_statuses,
    processes, specs,
  },
  utils::container::{
    cargo::{self, is_rollout_process_name},
    cargo_compiler::{
      LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_KIND,
      LABEL_REPLICA, LABEL_REPLICA_ORDINAL, LABEL_ROLE,
    },
  },
  vars,
};

use cargo_bootstrap::{
  CargoBootstrapProcess, ReconstructedCargo, observed_actual_status,
  reconstruct_cargo,
};

fn reattach_cargo_process(
  conn: &mut PgConnection,
  cargo_key: &str,
  process_key: &str,
  process_name: &str,
  labels: &HashMap<String, String>,
  node_name: &str,
) -> TransactionResult<()> {
  if is_rollout_process_name(process_name) {
    return Ok(());
  }
  if labels.get(LABEL_CARGO_KEY).map(String::as_str) != Some(cargo_key) {
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo process {process_key} labels do not identify expected Cargo {cargo_key:?}"
      ),
    ).into());
  }
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
    .parse::<CargoReplicaProcessRole>()
    .map_err(IoError::from)?;
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
  let replica = match cargo_replicas::table
    .find(replica_key)
    .for_update()
    .first::<CargoReplicaDb>(conn)
    .optional()?
  {
    Some(replica) => replica,
    None => diesel::insert_into(cargo_replicas::table)
      .values(NewCargoReplicaDb {
        key: replica_key,
        cargo_key: cargo_key.to_owned(),
        ordinal,
        node_name: Some(node_name.to_owned()),
      })
      .get_result::<CargoReplicaDb>(conn)?,
  };
  if replica.cargo_key != cargo_key
    || replica.ordinal != ordinal
    || replica.node_name.as_deref() != Some(node_name)
  {
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo process {process_key} does not match replica {replica_key} assignment"
      ),
    ).into());
  }
  let mapping = match cargo_replica_processes::table
    .filter(cargo_replica_processes::replica_key.eq(replica_key))
    .filter(cargo_replica_processes::role.eq(role))
    .filter(cargo_replica_processes::container_name.eq(container_name))
    .for_update()
    .first::<CargoReplicaProcessDb>(conn)
    .optional()?
  {
    Some(mapping) => mapping,
    None => diesel::insert_into(cargo_replica_processes::table)
      .values(NewCargoReplicaProcessDb::new(
        replica_key,
        container_name,
        role,
        essential,
      ))
      .get_result::<CargoReplicaProcessDb>(conn)?,
  };
  if mapping
    .process_key
    .as_deref()
    .is_some_and(|attached| attached != process_key)
  {
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo mapping {} is already attached to process {:?}, not observed process {process_key}",
        mapping.key, mapping.process_key
      ),
    ).into());
  }
  diesel::update(cargo_replica_processes::table.find(mapping.key))
    .set((
      cargo_replica_processes::process_key.eq(Some(process_key)),
      cargo_replica_processes::essential.eq(essential),
      cargo_replica_processes::updated_at.eq(chrono::Utc::now().naive_utc()),
    ))
    .execute(conn)?;
  Ok(())
}

fn sync_cargo_status(
  conn: &mut PgConnection,
  cargo_key: &str,
  actual: ObjPsStatusKind,
) -> TransactionResult<()> {
  let status = object_process_statuses::table
    .find(cargo_key)
    .for_update()
    .first::<ObjPsStatusDb>(conn)?;
  diesel::update(object_process_statuses::table.find(cargo_key))
    .set((
      object_process_statuses::wanted.eq(ObjPsStatusKind::Start.to_string()),
      object_process_statuses::prev_wanted.eq(status.wanted),
      object_process_statuses::actual.eq(actual.to_string()),
      object_process_statuses::prev_actual.eq(status.actual),
      object_process_statuses::updated_at.eq(chrono::Utc::now().naive_utc()),
    ))
    .execute(conn)?;
  Ok(())
}

fn persist_reconstructed_mappings(
  conn: &mut PgConnection,
  cargo_key: &str,
  reconstructed: &ReconstructedCargo,
  node_name: &str,
) -> TransactionResult<()> {
  let observed_replicas = reconstructed
    .processes
    .iter()
    .map(|process| (process.replica_ordinal, process.replica_key))
    .collect::<BTreeMap<_, _>>();
  let mut replicas = cargo_replicas::table
    .filter(cargo_replicas::cargo_key.eq(cargo_key))
    .order(cargo_replicas::ordinal.asc())
    .for_update()
    .load::<CargoReplicaDb>(conn)?;
  for (ordinal, replica_key) in observed_replicas {
    let by_key = replicas
      .iter()
      .find(|replica| replica.key == replica_key)
      .cloned();
    let by_ordinal = replicas
      .iter()
      .find(|replica| replica.ordinal == ordinal)
      .cloned();
    let replica = match (by_key, by_ordinal) {
      (Some(by_key), Some(by_ordinal)) if by_key.key == by_ordinal.key => {
        by_key
      }
      (None, None) => {
        let replica = diesel::insert_into(cargo_replicas::table)
          .values(NewCargoReplicaDb {
            key: replica_key,
            cargo_key: cargo_key.to_owned(),
            ordinal,
            node_name: Some(node_name.to_owned()),
          })
          .get_result::<CargoReplicaDb>(conn)?;
        replicas.push(replica.clone());
        replica
      }
      _ => {
        return Err(IoError::other(
          "CargoBootstrap",
          &format!(
            "Cargo {cargo_key:?} observed replica {replica_key} at ordinal {ordinal}, which conflicts with persisted replica identity"
          ),
        ).into());
      }
    };
    if replica.cargo_key != cargo_key
      || replica.node_name.as_deref() != Some(node_name)
    {
      return Err(IoError::other(
        "CargoBootstrap",
        &format!(
          "Cargo {cargo_key:?} observed replica {replica_key} does not match its persisted local assignment"
        ),
      ).into());
    }
  }

  for process in &reconstructed.processes {
    let mapping = match cargo_replica_processes::table
      .filter(cargo_replica_processes::replica_key.eq(process.replica_key))
      .filter(cargo_replica_processes::role.eq(process.role))
      .filter(
        cargo_replica_processes::container_name.eq(&process.container_name),
      )
      .for_update()
      .first::<CargoReplicaProcessDb>(conn)
      .optional()?
    {
      Some(mapping) => mapping,
      None => diesel::insert_into(cargo_replica_processes::table)
        .values(NewCargoReplicaProcessDb::new(
          process.replica_key,
          &process.container_name,
          process.role,
          process.essential,
        ))
        .get_result::<CargoReplicaProcessDb>(conn)?,
    };
    if mapping.essential != process.essential {
      return Err(IoError::other(
        "CargoBootstrap",
        &format!(
          "Cargo mapping {} contradicts the observed essential marker for process {:?}",
          mapping.key, process.process_key
        ),
      ).into());
    }
    if mapping
      .process_key
      .as_deref()
      .is_some_and(|attached| attached != process.process_key)
    {
      return Err(IoError::other(
        "CargoBootstrap",
        &format!(
          "Cargo mapping {} is already attached to process {:?}, not observed process {:?}",
          mapping.key, mapping.process_key, process.process_key
        ),
      ).into());
    }
    diesel::update(cargo_replica_processes::table.find(mapping.key))
      .set((
        cargo_replica_processes::process_key.eq(Some(&process.process_key)),
        cargo_replica_processes::essential.eq(process.essential),
        cargo_replica_processes::updated_at.eq(chrono::Utc::now().naive_utc()),
      ))
      .execute(conn)?;
  }
  Ok(())
}

/// An initial installer generation has no accepted-operation identity. Even
/// partial labels must not be mistaken for an adoptable bootstrap container.
fn bootstrap_labels_are_unowned(labels: &HashMap<String, String>) -> bool {
  [
    cargo::LABEL_INCARNATION,
    cargo::LABEL_SPEC,
    cargo::LABEL_OPERATION,
  ]
  .iter()
  .all(|label| !labels.contains_key(*label))
}

async fn persist_bootstrap_observations(
  expected: &Cargo,
  incarnation: uuid::Uuid,
  observed: &[CargoBootstrapProcess],
  reconstructed: Option<&ReconstructedCargo>,
  guard: Arc<tokio::sync::OwnedMutexGuard<()>>,
  state: &SystemState,
) -> IoResult<()> {
  if observed
    .iter()
    .any(|process| !bootstrap_labels_are_unowned(&process.labels))
  {
    return Err(IoError::invalid_data(
      "CargoBootstrap",
      "Operation-owned or partially labelled Cargo runtime cannot be adopted",
    ));
  }
  let replacement = if let Some(reconstructed) = reconstructed {
    let current = SpecDb::read_by_pk(&expected.spec.key, &state.inner.pool)
      .await?
      .try_to_cargo_spec()?;
    if current != reconstructed.spec {
      let mut spec = SpecDb::try_from_cargo_spec(
        &expected.spec.cargo_key,
        &format!("v{}", vars::VERSION),
        &reconstructed.spec,
      )?;
      spec.incarnation_key = Some(incarnation);
      Some(spec)
    } else {
      None
    }
  } else {
    None
  };
  let expected = expected.clone();
  let observed = observed.to_vec();
  let reconstructed = reconstructed.cloned();
  let node_name = state.inner.config.hostname.clone();
  run_query_owned(&state.inner.pool, guard, move |conn| {
    transaction(conn, |conn| {
      lock_bootstrap_cargo(conn, &expected, incarnation)?;
      let key = &expected.spec.cargo_key;
      if let Some(spec) = &replacement {
        diesel::insert_into(specs::table)
          .values(spec)
          .execute(conn)?;
        diesel::update(cargoes::table.find(key))
          .set(cargoes::spec_key.eq(spec.key))
          .execute(conn)?;
      }
      if let Some(reconstructed) = &reconstructed {
        persist_reconstructed_mappings(conn, key, reconstructed, &node_name)?;
        sync_cargo_status(conn, key, reconstructed.actual.clone())?;
      } else {
        for process in &observed {
          reattach_cargo_process(
            conn,
            key,
            &process.process_key,
            &process.process_name,
            &process.labels,
            &node_name,
          )?;
        }
        if observed
          .iter()
          .any(|process| process.process_name == format!("{key}.c"))
          && let Some(actual) = observed_actual_status(&observed)
        {
          sync_cargo_status(conn, key, actual)?;
        }
      }
      Ok(())
    })
  })
  .await
}

async fn persist_reconstructed_cargo(
  cargo_key: &str,
  reconstructed: &ReconstructedCargo,
  guard: Arc<tokio::sync::OwnedMutexGuard<()>>,
  state: &SystemState,
) -> IoResult<()> {
  reconstructed.spec.validate().map_err(|error| {
    IoError::invalid_data(
      "CargoBootstrap",
      &format!("reconstructed Cargo {cargo_key:?} is invalid: {error}"),
    )
  })?;
  let resource_key = cargo_key.parse::<ResourceKey>().map_err(|error| {
    IoError::invalid_data("CargoBootstrap", &error.to_string())
  })?;
  register_namespace(resource_key.namespace(), state).await?;
  let spec = SpecDb::try_from_cargo_spec(
    cargo_key,
    &format!("v{}", vars::VERSION),
    &reconstructed.spec,
  )?;
  let status = ObjPsStatusPartial {
    key: cargo_key.to_owned(),
    wanted: ObjPsStatusKind::Create,
    prev_wanted: ObjPsStatusKind::Create,
    actual: ObjPsStatusKind::Create,
    prev_actual: ObjPsStatusKind::Create,
    health: ObjPsHealthStatusKind::Unknown,
  };
  let cargo = CargoDb::create_bootstrap_with_spec(
    cargo_key,
    &reconstructed.spec.name,
    resource_key.namespace(),
    spec,
    status,
    &state.inner.pool,
  )
  .await?;
  let incarnation = CargoDb::read_by_pk(cargo_key, &state.inner.pool)
    .await?
    .0
    .incarnation_key;
  persist_bootstrap_observations(
    &cargo,
    incarnation,
    &[],
    Some(reconstructed),
    guard,
    state,
  )
  .await
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

fn stale_process_ids(initial: Vec<String>, observed: &[String]) -> Vec<String> {
  let observed = observed.iter().map(String::as_str).collect::<HashSet<_>>();
  initial
    .into_iter()
    .filter(|id| !observed.contains(id.as_str()))
    .collect()
}

/// Rebuild observed process rows and adopt installer/bootstrap Cargoes whose
/// desired rows do not exist yet.
pub async fn sync_processes(state: &SystemState) -> IoResult<()> {
  log::info!("system::sync_processes: starting");
  // Bound cleanup to rows that existed before Docker inventory. An admitted
  // operation may insert new rows while the scan is in progress.
  let node_name = state.inner.config.hostname.clone();
  let initial_ids = run_query_owned(&state.inner.pool, (), move |conn| {
    transaction(conn, |conn| {
      Ok(
        processes::table
          .filter(processes::node_name.eq(&node_name))
          .select(processes::key)
          .load::<String>(conn)?,
      )
    })
  })
  .await?;
  let options = Some(ListContainersOptions {
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
  let mut observed_cargoes =
    BTreeMap::<String, Vec<CargoBootstrapProcess>>::new();
  for container_summary in containers {
    let labels = container_summary.labels.unwrap_or_default();
    let Some(kind) = labels.get(LABEL_KIND) else {
      continue;
    };
    let key = match kind.as_str() {
      "cargo" => labels.get(LABEL_CARGO_KEY),
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
      let observed = CargoBootstrapProcess::from_inspect(&container)?;
      observed_cargoes
        .entry(key.clone())
        .or_default()
        .push(observed);
    }
    ids.push(id);
  }
  for (cargo_key, observed) in observed_cargoes {
    let guard = Arc::new(cargo::lifecycle_lock(&cargo_key).lock_owned().await);
    if observed
      .iter()
      .any(|process| !bootstrap_labels_are_unowned(&process.labels))
    {
      log::debug!(
        "system::sync_processes: Cargo {cargo_key} has operation-owned runtime; keeping observations only"
      );
      continue;
    }
    match CargoDb::read_by_pk(&cargo_key, &state.inner.pool).await {
      Ok((live, spec, status)) => {
        let incarnation = live.incarnation_key;
        let cargo = CargoDb::transform((live, spec, status))?;
        if OperationDb::current("Cargo", &cargo_key, &state.inner.pool)
          .await?
          .is_some()
          || matches!(
            cargo.status.actual,
            ObjPsStatusKind::Starting
              | ObjPsStatusKind::Updating
              | ObjPsStatusKind::Destroying
              | ObjPsStatusKind::Stopping
          )
        {
          log::debug!(
            "system::sync_processes: Cargo {cargo_key} has lifecycle ownership; keeping observations only"
          );
          continue;
        }
        let installer_generation = observed
          .iter()
          .any(|process| process.process_name == format!("{cargo_key}.c"));
        let has_reconstruction_identity_labels = observed
          .iter()
          .filter(|process| !is_rollout_process_name(&process.process_name))
          .any(|process| {
            [
              LABEL_REPLICA,
              LABEL_REPLICA_ORDINAL,
              LABEL_ROLE,
              LABEL_CONTAINER,
              LABEL_ESSENTIAL,
              CARGO_CONTAINER_POSITION_LABEL,
            ]
            .iter()
            .any(|label| process.labels.contains_key(*label))
          });
        let reconstructed = if installer_generation
          && has_reconstruction_identity_labels
        {
          let reconstructed = reconstruct_cargo(&cargo_key, &observed)?
            .ok_or_else(|| {
              IoError::invalid_data(
                "CargoBootstrap",
                &format!("Cargo {cargo_key:?} has an installer process but no stable generation"),
              )
            })?;
          if !reconstructed.direct_installer_generation {
            return Err(IoError::invalid_data(
              "CargoBootstrap",
              &format!(
                "Cargo {cargo_key:?} has contradictory installer and reconciled runtime identities"
              ),
            ));
          }
          Some(reconstructed)
        } else {
          None
        };
        persist_bootstrap_observations(
          &cargo,
          incarnation,
          &observed,
          reconstructed.as_ref(),
          guard,
          state,
        )
        .await?;
      }
      Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
        let Some(reconstructed) = reconstruct_cargo(&cargo_key, &observed)?
        else {
          continue;
        };
        let source = if reconstructed.direct_installer_generation {
          "installer"
        } else {
          "stable runtime"
        };
        log::info!(
          "system::sync_processes: reconstructing bootstrap Cargo {cargo_key} from {source} observations across {} stable processes",
          reconstructed.processes.len()
        );
        persist_reconstructed_cargo(&cargo_key, &reconstructed, guard, state)
          .await?;
      }
      Err(error) => return Err(error),
    }
  }
  let stale_ids = stale_process_ids(initial_ids, &ids);
  if !stale_ids.is_empty() {
    let filter = GenericFilter::new()
      .r#where("key", GenericClause::In(stale_ids))
      .r#where(
        "node_name",
        GenericClause::Eq(state.inner.config.hostname.clone()),
      );
    ProcessDb::del_by(&filter, &state.inner.pool).await?;
  }
  log::info!("system::sync_processes: done");
  Ok(())
}

#[cfg(test)]
mod tests {
  use bollard_next::models::{
    ContainerInspectResponse, ContainerState, ContainerStateStatusEnum,
    HostConfig,
  };
  use nanocl_stubs::{
    cargo_spec::{CARGO_CONTAINER_POSITION_LABEL, Config as DockerConfig},
    process::{ProcessKind, ProcessPartial},
  };

  use crate::utils::{
    container::cargo_compiler::{
      LABEL_CARGO_KEY, LABEL_CARGO_NETWORK_MODE, LABEL_CONTAINER,
      LABEL_ESSENTIAL, LABEL_REPLICA, LABEL_REPLICA_ORDINAL, LABEL_ROLE,
    },
    tests::gen_default_test_system,
  };

  use super::*;

  fn bootstrap_process(
    cargo_key: &str,
    replica_key: uuid::Uuid,
    role: CargoReplicaProcessRole,
    logical_name: &str,
    position: Option<usize>,
    process_name: String,
    network_mode: String,
  ) -> CargoBootstrapProcess {
    let mut labels = HashMap::from([
      (LABEL_CARGO_KEY.to_owned(), cargo_key.to_owned()),
      (LABEL_REPLICA.to_owned(), replica_key.to_string()),
      (LABEL_REPLICA_ORDINAL.to_owned(), "0".to_owned()),
      (LABEL_ROLE.to_owned(), role.to_string()),
      (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
      (LABEL_ESSENTIAL.to_owned(), "true".to_owned()),
      (LABEL_CARGO_NETWORK_MODE.to_owned(), "nanoclbr0".to_owned()),
    ]);
    if let Some(position) = position {
      labels.insert(
        CARGO_CONTAINER_POSITION_LABEL.to_owned(),
        position.to_string(),
      );
    }
    CargoBootstrapProcess {
      process_key: uuid::Uuid::new_v4().to_string(),
      process_name,
      labels: labels.clone(),
      config: DockerConfig {
        image: Some(
          if role == CargoReplicaProcessRole::Sandbox {
            "pause:latest"
          } else {
            "alpine:latest"
          }
          .to_owned(),
        ),
        labels: Some(labels),
        host_config: Some(HostConfig {
          network_mode: Some(network_mode),
          ..Default::default()
        }),
        ..Default::default()
      },
      status: Some(ContainerStateStatusEnum::RUNNING),
    }
  }

  async fn insert_observed_process(
    cargo_key: &str,
    process: &CargoBootstrapProcess,
    state: &SystemState,
  ) {
    let data = serde_json::to_value(ContainerInspectResponse {
      id: Some(process.process_key.clone()),
      name: Some(format!("/{}", process.process_name)),
      state: Some(ContainerState {
        status: process.status,
        running: Some(
          process.status == Some(ContainerStateStatusEnum::RUNNING),
        ),
        ..Default::default()
      }),
      ..Default::default()
    })
    .unwrap();
    ProcessDb::create_from(
      &ProcessPartial {
        key: process.process_key.clone(),
        name: process.process_name.clone(),
        kind: ProcessKind::Cargo,
        data,
        node_name: state.inner.config.hostname.clone(),
        kind_key: cargo_key.to_owned(),
        created_at: Some(chrono::Utc::now().naive_utc()),
      },
      &state.inner.pool,
    )
    .await
    .unwrap();
  }

  #[test]
  fn startup_cleanup_only_removes_missing_initial_processes() {
    let initial = vec!["present".into(), "gone".into()];
    let observed = vec!["present".into(), "new-during-inventory".into()];
    assert_eq!(stale_process_ids(initial, &observed), ["gone"]);
    assert!(stale_process_ids(Vec::new(), &observed).is_empty());
    assert_eq!(stale_process_ids(vec!["gone".into()], &[]), ["gone"]);
  }

  #[test]
  fn bootstrap_adoption_rejects_complete_and_partial_operation_identity() {
    let mut labels = HashMap::from([(
      LABEL_CARGO_KEY.to_owned(),
      "system.installer".to_owned(),
    )]);
    assert!(bootstrap_labels_are_unowned(&labels));
    for label in [
      cargo::LABEL_INCARNATION,
      cargo::LABEL_SPEC,
      cargo::LABEL_OPERATION,
    ] {
      labels.insert(label.to_owned(), uuid::Uuid::nil().to_string());
      assert!(!bootstrap_labels_are_unowned(&labels), "{label}");
      labels.remove(label);
    }
    labels.extend([
      (
        cargo::LABEL_INCARNATION.to_owned(),
        uuid::Uuid::nil().to_string(),
      ),
      (cargo::LABEL_SPEC.to_owned(), uuid::Uuid::nil().to_string()),
      (
        cargo::LABEL_OPERATION.to_owned(),
        uuid::Uuid::nil().to_string(),
      ),
    ]);
    assert!(!bootstrap_labels_are_unowned(&labels));
  }

  /// DB-backed startup coverage. This is deliberately excluded from the pure
  /// reconstruction filter and is run manually with the Nanocld test store.
  #[ntex::test]
  async fn bootstrap_persistence_adopts_final_spec_and_runtime_mappings() {
    let system = gen_default_test_system().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let cargo_name = format!("bootstrap-sync-{}", &suffix[..8]);
    let cargo_key = format!("system.{cargo_name}");
    let replica_key = uuid::Uuid::new_v4();
    let sandbox = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::Sandbox,
      "_sandbox",
      None,
      format!("sandbox-{cargo_key}-r0.c"),
      "nanoclbr0".to_owned(),
    );
    let sandbox_mode = format!("container:{}", sandbox.process_key);
    let api = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "api",
      Some(0),
      format!("{cargo_key}-r0-api.c"),
      sandbox_mode.clone(),
    );
    let sidecar = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "sidecar",
      Some(1),
      format!("{cargo_key}-r0-sidecar.c"),
      sandbox_mode.clone(),
    );
    let mut candidate = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "api",
      Some(0),
      format!("{cargo_key}-r0-api.candidate.c"),
      sandbox_mode.clone(),
    );
    candidate.labels.remove(CARGO_CONTAINER_POSITION_LABEL);
    let mut retained = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "sidecar",
      Some(1),
      format!("{cargo_key}-r0-sidecar.tmp.c"),
      sandbox_mode,
    );
    retained.labels.remove(CARGO_CONTAINER_POSITION_LABEL);
    let candidate_key = candidate.process_key.clone();
    let retained_key = retained.process_key.clone();
    let observed = vec![sandbox, api, sidecar, candidate, retained];
    for process in &observed {
      insert_observed_process(&cargo_key, process, &system.state).await;
    }

    let reconstructed =
      reconstruct_cargo(&cargo_key, &observed).unwrap().unwrap();
    persist_reconstructed_cargo(
      &cargo_key,
      &reconstructed,
      Arc::new(cargo::lifecycle_lock(&cargo_key).lock_owned().await),
      &system.state,
    )
    .await
    .unwrap();

    let cargo =
      CargoDb::transform_read_by_pk(&cargo_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(cargo.spec.name, cargo_name);
    assert_eq!(cargo.spec.replicas, 1);
    assert!(cargo.spec.init_containers.is_empty());
    assert_eq!(
      cargo
        .spec
        .containers
        .iter()
        .map(|container| container.name.as_str())
        .collect::<Vec<_>>(),
      ["api", "sidecar"]
    );
    assert!(
      cargo
        .spec
        .containers
        .iter()
        .all(|container| container.name != "_sandbox")
    );

    let replicas =
      CargoReplicaDb::list_by_cargo(&cargo_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(replicas.len(), 1);
    assert_eq!(replicas[0].key, replica_key);
    let mappings = CargoReplicaProcessDb::list_by_replica(
      replica_key,
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    assert_eq!(mappings.len(), 3);
    assert!(mappings.iter().all(|mapping| mapping.process_key.is_some()));
    for pending_key in [&candidate_key, &retained_key] {
      assert!(
        mappings
          .iter()
          .all(|mapping| mapping.process_key.as_deref() != Some(pending_key))
      );
    }

    // Simulate a later startup attachment pass. Stable mappings remain
    // coherent and the pending process cannot steal the active app mapping.
    let incarnation = CargoDb::read_by_pk(&cargo_key, &system.state.inner.pool)
      .await
      .unwrap()
      .0
      .incarnation_key;
    persist_bootstrap_observations(
      &cargo,
      incarnation,
      &observed,
      None,
      Arc::new(cargo::lifecycle_lock(&cargo_key).lock_owned().await),
      &system.state,
    )
    .await
    .unwrap();
    let cargo =
      CargoDb::transform_read_by_pk(&cargo_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(cargo.spec.containers.len(), 2);
    assert!(
      cargo
        .spec
        .containers
        .iter()
        .all(|container| container.name != "_sandbox")
    );

    CargoDb::del_by_pk(&cargo_key, &system.state.inner.pool)
      .await
      .unwrap();
    for process in &observed {
      ProcessDb::del_by_pk(&process.process_key, &system.state.inner.pool)
        .await
        .unwrap();
    }
  }

  /// DB-backed startup coverage. Run manually with the Nanocld test store.
  #[ntex::test]
  async fn bootstrap_persistence_adopts_direct_single_application_mapping() {
    let system = gen_default_test_system().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let cargo_name = format!("bootstrap-direct-{}", &suffix[..8]);
    let cargo_key = format!("system.{cargo_name}");
    let replica_key = uuid::Uuid::new_v4();
    let api = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "api",
      Some(0),
      format!("{cargo_key}-r0-api.c"),
      "nanoclbr0".to_owned(),
    );
    insert_observed_process(&cargo_key, &api, &system.state).await;

    let reconstructed =
      reconstruct_cargo(&cargo_key, std::slice::from_ref(&api))
        .unwrap()
        .unwrap();
    persist_reconstructed_cargo(
      &cargo_key,
      &reconstructed,
      Arc::new(cargo::lifecycle_lock(&cargo_key).lock_owned().await),
      &system.state,
    )
    .await
    .unwrap();

    let cargo =
      CargoDb::transform_read_by_pk(&cargo_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(cargo.spec.name, cargo_name);
    assert_eq!(cargo.spec.replicas, 1);
    assert_eq!(cargo.spec.containers.len(), 1);
    assert_eq!(cargo.spec.containers[0].name, "api");
    assert!(cargo.spec.init_containers.is_empty());

    let replicas =
      CargoReplicaDb::list_by_cargo(&cargo_key, &system.state.inner.pool)
        .await
        .unwrap();
    assert_eq!(replicas.len(), 1);
    assert_eq!(replicas[0].key, replica_key);
    let mappings = CargoReplicaProcessDb::list_by_replica(
      replica_key,
      &system.state.inner.pool,
    )
    .await
    .unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].role, CargoReplicaProcessRole::App);
    assert_eq!(mappings[0].container_name, "api");
    assert_eq!(
      mappings[0].process_key.as_deref(),
      Some(api.process_key.as_str())
    );
    assert!(mappings.iter().all(|mapping| {
      mapping.role != CargoReplicaProcessRole::Sandbox
        && mapping.container_name != "_sandbox"
    }));

    CargoDb::del_by_pk(&cargo_key, &system.state.inner.pool)
      .await
      .unwrap();
    ProcessDb::del_by_pk(&api.process_key, &system.state.inner.pool)
      .await
      .unwrap();
  }
}
