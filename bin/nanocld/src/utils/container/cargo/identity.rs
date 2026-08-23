use std::collections::{HashMap, HashSet};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{node::CargoReplicaTask, process::Process};

use crate::{
  models::CargoReplicaProcessRole,
  utils::container::cargo_compiler::{
    LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_REPLICA, LABEL_REPLICA_ORDINAL,
    LABEL_ROLE,
  },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CargoProcessIdentity {
  pub(super) replica_key: uuid::Uuid,
  pub(super) ordinal: i32,
  pub(super) role: CargoReplicaProcessRole,
  pub(super) container_name: String,
  pub(super) essential: bool,
}

pub(super) fn replica_ordinal(task: &CargoReplicaTask) -> IoResult<u32> {
  u32::try_from(task.ordinal).map_err(|_| {
    IoError::other(
      "CargoRuntime",
      &format!(
        "Cargo replica {} has invalid negative ordinal {}",
        task.key, task.ordinal
      ),
    )
  })
}

pub(super) fn process_identity(
  process: &Process,
) -> IoResult<CargoProcessIdentity> {
  let labels = process
    .data
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref())
    .ok_or_else(|| {
      IoError::other(
        "CargoRuntimeIdentity",
        &format!("Cargo process {} has no Docker labels", process.key),
      )
    })?;
  let value = |label: &str| {
    labels
      .get(label)
      .filter(|value| !value.is_empty())
      .ok_or_else(|| {
        IoError::other(
          "CargoRuntimeIdentity",
          &format!("Cargo process {} is missing label {label}", process.key),
        )
      })
  };
  let replica_key =
    value(LABEL_REPLICA)?
      .parse::<uuid::Uuid>()
      .map_err(|error| {
        IoError::other(
          "CargoRuntimeIdentity",
          &format!(
            "Cargo process {} has invalid replica UUID: {error}",
            process.key
          ),
        )
      })?;
  let ordinal =
    value(LABEL_REPLICA_ORDINAL)?
      .parse::<i32>()
      .map_err(|error| {
        IoError::other(
          "CargoRuntimeIdentity",
          &format!(
            "Cargo process {} has invalid replica ordinal: {error}",
            process.key
          ),
        )
      })?;
  let role = value(LABEL_ROLE)?.parse::<CargoReplicaProcessRole>()?;
  let container_name = value(LABEL_CONTAINER)?.clone();
  let essential = value(LABEL_ESSENTIAL)?.parse::<bool>().map_err(|error| {
    IoError::other(
      "CargoRuntimeIdentity",
      &format!(
        "Cargo process {} has invalid essential label: {error}",
        process.key
      ),
    )
  })?;
  Ok(CargoProcessIdentity {
    replica_key,
    ordinal,
    role,
    container_name,
    essential,
  })
}

pub(super) fn validate_observed_identities(
  cargo_key: &str,
  requested: &[CargoReplicaTask],
  processes: &[Process],
  local_node: &str,
) -> IoResult<()> {
  let requested = requested
    .iter()
    .map(|replica| (replica.key, replica.ordinal))
    .collect::<HashMap<_, _>>();
  let mut observed = HashSet::new();
  for process in processes {
    let identity = process_identity(process)?;
    if !observed.insert((
      identity.replica_key,
      identity.role,
      identity.container_name.clone(),
    )) {
      return Err(IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo {cargo_key:?} has duplicate observed {:?} process {:?} for replica {}",
          identity.role, identity.container_name, identity.replica_key
        ),
      ));
    }
    let Some(expected_ordinal) = requested.get(&identity.replica_key) else {
      continue;
    };
    if identity.ordinal != *expected_ordinal {
      return Err(IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo {cargo_key:?} replica {} was requested with ordinal {} but process {} declares ordinal {}",
          identity.replica_key, expected_ordinal, process.key, identity.ordinal
        ),
      ));
    }
    if process.node_name != local_node {
      return Err(IoError::other(
        "CargoRuntimeIdentity",
        &format!(
          "Cargo {cargo_key:?} replica {} is requested on node {local_node:?} but process {} is observed on node {:?}",
          identity.replica_key, process.key, process.node_name
        ),
      ));
    }
  }
  Ok(())
}

pub(super) fn is_rollout_process_name(name: &str) -> bool {
  name.starts_with("tmp-") || name.starts_with("candidate-")
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::models::{
    ContainerConfig, ContainerInspectResponse, ContainerState,
  };
  use nanocl_stubs::process::{Process, ProcessKind};

  use super::*;

  fn labelled_process() -> Process {
    Process {
      key: "docker-id".to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      name: "global.api-r7-app-test.c".to_owned(),
      kind: ProcessKind::Cargo,
      node_name: "node-a".to_owned(),
      kind_key: "global.api".to_owned(),
      data: ContainerInspectResponse {
        config: Some(ContainerConfig {
          labels: Some(HashMap::from([
            (
              LABEL_REPLICA.to_owned(),
              "9bb4a315-c6c6-4200-bb79-08e66c42e2bd".to_owned(),
            ),
            (LABEL_REPLICA_ORDINAL.to_owned(), "7".to_owned()),
            (LABEL_ROLE.to_owned(), "app".to_owned()),
            (LABEL_CONTAINER.to_owned(), "api".to_owned()),
            (LABEL_ESSENTIAL.to_owned(), "true".to_owned()),
          ])),
          ..Default::default()
        }),
        state: Some(ContainerState {
          running: Some(true),
          ..Default::default()
        }),
        ..Default::default()
      },
    }
  }

  #[test]
  fn observed_process_identity_uses_canonical_replica_labels() {
    let identity = process_identity(&labelled_process()).unwrap();
    assert_eq!(
      identity.replica_key,
      uuid::Uuid::parse_str("9bb4a315-c6c6-4200-bb79-08e66c42e2bd").unwrap()
    );
    assert_eq!(identity.ordinal, 7);
    assert_eq!(identity.role, CargoReplicaProcessRole::App);
    assert_eq!(identity.container_name, "api");
    assert!(identity.essential);
  }

  #[test]
  fn missing_replica_identity_is_an_internal_error() {
    let mut process = labelled_process();
    process
      .data
      .config
      .as_mut()
      .unwrap()
      .labels
      .as_mut()
      .unwrap()
      .remove(LABEL_REPLICA);
    let error = process_identity(&process).unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Other);
    assert!(error.to_string().contains(LABEL_REPLICA));
  }
}
