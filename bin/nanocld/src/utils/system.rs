mod cargo_bootstrap;

use std::collections::{BTreeMap, HashMap};

use nanocl_error::io::{FromIo, IoError, IoResult};

use bollard_next::{
  container::{InspectContainerOptions, ListContainersOptions},
  service::ContainerInspectResponse,
};
use nanocl_stubs::{
  cargo_spec::CARGO_CONTAINER_POSITION_LABEL,
  generic::{GenericClause, GenericFilter},
  namespace::NamespacePartial,
  process::ProcessPartial,
  resource_key::ResourceKey,
  system::ObjPsStatusKind,
};

use crate::{
  models::{
    CargoDb, CargoObjCreateIn, CargoReplicaDb, CargoReplicaProcessDb,
    CargoReplicaProcessRole, NamespaceDb, NewCargoReplicaDb,
    NewCargoReplicaProcessDb, ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb,
    ProcessUpdateDb, SpecDb, SystemState,
  },
  objects::generic::ObjCreate,
  repositories::generic::*,
  utils::container::cargo_compiler::{
    LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_KIND,
    LABEL_REPLICA, LABEL_REPLICA_ORDINAL, LABEL_ROLE,
  },
  vars,
};

use cargo_bootstrap::{
  CargoBootstrapProcess, ReconstructedCargo, observed_actual_status,
  reconstruct_cargo,
};

async fn reattach_cargo_process(
  cargo_key: &str,
  process_key: &str,
  process_name: &str,
  labels: &HashMap<String, String>,
  state: &SystemState,
) -> IoResult<()> {
  if process_name.starts_with("tmp-") || process_name.starts_with("candidate-")
  {
    return Ok(());
  }
  if labels.get(LABEL_CARGO_KEY).map(String::as_str) != Some(cargo_key) {
    return Err(IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo process {process_key} labels do not identify expected Cargo {cargo_key:?}"
      ),
    ));
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
  let replica = match CargoReplicaDb::get(replica_key, &state.inner.pool).await
  {
    Ok(replica) => replica,
    Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
      CargoReplicaDb::create(
        NewCargoReplicaDb {
          key: replica_key,
          cargo_key: cargo_key.to_owned(),
          ordinal,
          node_name: Some(state.inner.config.hostname.clone()),
        },
        &state.inner.pool,
      )
      .await?
    }
    Err(error) => return Err(error),
  };
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

async fn sync_cargo_status(
  cargo_key: &str,
  actual: ObjPsStatusKind,
  state: &SystemState,
) -> IoResult<()> {
  let status = ObjPsStatusDb::read_by_pk(cargo_key, &state.inner.pool).await?;
  ObjPsStatusDb::update_pk(
    cargo_key,
    ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(actual.to_string()),
      prev_actual: Some(status.actual),
      ..Default::default()
    },
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

async fn persist_reconstructed_mappings(
  cargo_key: &str,
  reconstructed: &ReconstructedCargo,
  state: &SystemState,
) -> IoResult<()> {
  let observed_replicas = reconstructed
    .processes
    .iter()
    .map(|process| (process.replica_ordinal, process.replica_key))
    .collect::<BTreeMap<_, _>>();
  let mut replicas =
    CargoReplicaDb::list_by_cargo(cargo_key, &state.inner.pool).await?;
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
        let replica = CargoReplicaDb::create(
          NewCargoReplicaDb {
            key: replica_key,
            cargo_key: cargo_key.to_owned(),
            ordinal,
            node_name: Some(state.inner.config.hostname.clone()),
          },
          &state.inner.pool,
        )
        .await?;
        replicas.push(replica.clone());
        replica
      }
      _ => {
        return Err(IoError::other(
          "CargoBootstrap",
          &format!(
            "Cargo {cargo_key:?} observed replica {replica_key} at ordinal {ordinal}, which conflicts with persisted replica identity"
          ),
        ));
      }
    };
    if replica.cargo_key != cargo_key
      || replica.node_name.as_deref()
        != Some(state.inner.config.hostname.as_str())
    {
      return Err(IoError::other(
        "CargoBootstrap",
        &format!(
          "Cargo {cargo_key:?} observed replica {replica_key} does not match its persisted local assignment"
        ),
      ));
    }
  }

  for process in &reconstructed.processes {
    let mapping = match CargoReplicaProcessDb::find(
      process.replica_key,
      process.role,
      &process.container_name,
      &state.inner.pool,
    )
    .await
    {
      Ok(mapping) => mapping,
      Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => {
        CargoReplicaProcessDb::create(
          NewCargoReplicaProcessDb::new(
            process.replica_key,
            &process.container_name,
            process.role,
            process.essential,
          ),
          &state.inner.pool,
        )
        .await?
      }
      Err(error) => return Err(error),
    };
    if mapping.essential != process.essential {
      return Err(IoError::other(
        "CargoBootstrap",
        &format!(
          "Cargo mapping {} contradicts the observed essential marker for process {:?}",
          mapping.key, process.process_key
        ),
      ));
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
      ));
    }
    CargoReplicaProcessDb::set_processes(
      vec![(
        mapping.key,
        Some(process.process_key.clone()),
        process.essential,
      )],
      &state.inner.pool,
    )
    .await?;
  }
  Ok(())
}

async fn persist_reconstructed_cargo(
  cargo_key: &str,
  reconstructed: &ReconstructedCargo,
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
  let object = CargoObjCreateIn {
    namespace: resource_key.namespace().to_owned(),
    spec: reconstructed.spec.clone(),
    version: format!("v{}", vars::VERSION),
  };
  CargoDb::fn_create_obj(&object, state).await?;
  persist_reconstructed_mappings(cargo_key, reconstructed, state).await?;
  sync_cargo_status(cargo_key, reconstructed.actual.clone(), state).await?;
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

/// Rebuild observed process rows and adopt installer/bootstrap Cargoes whose
/// desired rows do not exist yet.
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
    match CargoDb::transform_read_by_pk(&cargo_key, &state.inner.pool).await {
      Ok(cargo) => {
        if matches!(
          cargo.status.actual,
          ObjPsStatusKind::Starting
            | ObjPsStatusKind::Updating
            | ObjPsStatusKind::Destroying
            | ObjPsStatusKind::Stopping
        ) {
          log::warn!(
            "system::sync_processes: Cargo {cargo_key} has in-progress lifecycle state {}; startup inventory will not recover it",
            cargo.status.actual
          );
          continue;
        }
        let installer_generation = observed
          .iter()
          .any(|process| process.process_name == format!("{cargo_key}.c"));
        let has_reconstruction_identity_labels = observed
          .iter()
          .filter(|process| {
            !process.process_name.starts_with("tmp-")
              && !process.process_name.starts_with("candidate-")
          })
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
        if installer_generation && has_reconstruction_identity_labels {
          let reconstructed = reconstruct_cargo(&cargo_key, &observed)?
            .ok_or_else(|| {
              IoError::invalid_data(
                "CargoBootstrap",
                &format!(
                  "Cargo {cargo_key:?} has an installer process but no stable generation"
                ),
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
          let current = SpecDb::read_by_pk(&cargo.spec.key, &state.inner.pool)
            .await?
            .try_to_cargo_spec()?;
          if current != reconstructed.spec {
            CargoDb::update_from_spec(
              &cargo_key,
              &reconstructed.spec,
              &format!("v{}", vars::VERSION),
              &state.inner.pool,
            )
            .await?;
          }
          persist_reconstructed_mappings(&cargo_key, &reconstructed, state)
            .await?;
          sync_cargo_status(&cargo_key, reconstructed.actual, state).await?;
        } else {
          for process in &observed {
            reattach_cargo_process(
              &cargo_key,
              &process.process_key,
              &process.process_name,
              &process.labels,
              state,
            )
            .await?;
          }
          if installer_generation
            && let Some(actual) = observed_actual_status(&observed)
          {
            sync_cargo_status(&cargo_key, actual, state).await?;
          }
        }
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
        persist_reconstructed_cargo(&cargo_key, &reconstructed, state).await?;
      }
      Err(error) => return Err(error),
    }
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
      LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA,
      LABEL_REPLICA_ORDINAL, LABEL_ROLE,
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
  ) -> CargoBootstrapProcess {
    let mut labels = HashMap::from([
      (LABEL_CARGO_KEY.to_owned(), cargo_key.to_owned()),
      (LABEL_REPLICA.to_owned(), replica_key.to_string()),
      (LABEL_REPLICA_ORDINAL.to_owned(), "0".to_owned()),
      (LABEL_ROLE.to_owned(), role.to_string()),
      (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
      (LABEL_ESSENTIAL.to_owned(), "true".to_owned()),
    ]);
    if let Some(position) = position {
      labels.insert(
        CARGO_CONTAINER_POSITION_LABEL.to_owned(),
        position.to_string(),
      );
    }
    let network_mode = if role == CargoReplicaProcessRole::Sandbox {
      "nanoclbr0".to_owned()
    } else {
      "container:bootstrap-sandbox".to_owned()
    };
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
    );
    let api = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "api",
      Some(0),
      format!("{cargo_key}-r0-api.c"),
    );
    let sidecar = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "sidecar",
      Some(1),
      format!("{cargo_key}-r0-sidecar.c"),
    );
    let mut candidate = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "api",
      Some(0),
      format!("candidate-{cargo_key}-r0-api.c"),
    );
    candidate.labels.remove(CARGO_CONTAINER_POSITION_LABEL);
    let mut retained = bootstrap_process(
      &cargo_key,
      replica_key,
      CargoReplicaProcessRole::App,
      "sidecar",
      Some(1),
      format!("tmp-{cargo_key}-r0-sidecar.c"),
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
    persist_reconstructed_cargo(&cargo_key, &reconstructed, &system.state)
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
    for process in &observed {
      reattach_cargo_process(
        &cargo_key,
        &process.process_key,
        &process.process_name,
        &process.labels,
        &system.state,
      )
      .await
      .unwrap();
    }
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
}
