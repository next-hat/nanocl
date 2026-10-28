use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
  time::Duration,
};

use bollard_next::query_parameters::ListContainersOptions;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::system::NativeEventAction;

use crate::{
  models::{
    CargoDb, CargoReplicaDb, CargoReplicaProcessDb, OperationDb, ProcessDb,
    SystemState,
  },
  repositories::generic::*,
  utils,
};

fn deferred(reason: &str) -> IoError {
  IoError::interrupted("Cargo recovery", reason)
}

/// Losing a coordinator file lock cannot prove that a remote worker stopped.
/// Recovery only observes local, fully associated runtime IDs; it never replays
/// a rollout, promotes candidates, or removes containers.
pub(crate) async fn recover_one(
  candidate: &OperationDb,
  state: &SystemState,
) -> IoResult<bool> {
  let Ok(guard) =
    super::lifecycle_lock(&candidate.resource_key).try_lock_owned()
  else {
    return Ok(false);
  };
  let Some(current) =
    OperationDb::current("Cargo", &candidate.resource_key, &state.inner.pool)
      .await?
  else {
    return Ok(false);
  };
  if current != *candidate || current.outcome.is_some() {
    return Ok(false);
  }
  let Some(owner) = utils::operation::abandoned_owner(
    &candidate.owner(),
    &state.inner.config.hostname,
    &state.inner.config.state_dir,
  )?
  else {
    return Ok(false);
  };
  let lease = (Arc::new(guard), owner);
  if !CargoDb::validate_operation(candidate, lease.clone(), &state.inner.pool)
    .await?
  {
    return Ok(false);
  }
  let cargo =
    CargoDb::transform_read_by_pk(&candidate.resource_key, &state.inner.pool)
      .await?;
  let observation = async {
    let replicas =
      CargoReplicaDb::list_by_cargo(&candidate.resource_key, &state.inner.pool)
        .await?;
    if replicas.iter().any(|replica| {
      replica.node_name.as_deref().is_some_and(|node| {
        node != state.inner.config.hostname && node != "init-test.nanocl.io"
      })
    }) {
      return Err(deferred(
        "Remote execution remains uncertain after coordinator ownership loss",
      ));
    }
    let processes = ProcessDb::read_by_kind_key(
      &candidate.resource_key,
      None,
      &state.inner.pool,
    )
    .await?;
    if processes.iter().any(|process| {
      process.node_name != state.inner.config.hostname
        || super::is_rollout_process_name(&process.name)
    }) {
      return Err(deferred(
        "Retained, candidate, or nonlocal runtime requires explicit reconciliation",
      ));
    }
    let ids = processes
      .iter()
      .map(|process| process.key.as_str())
      .collect::<HashSet<_>>();
    let mut mapped_ids = HashSet::new();
    for replica in &replicas {
      for mapping in
        CargoReplicaProcessDb::list_by_replica(replica.key, &state.inner.pool)
          .await?
      {
        if let Some(id) = mapping.process_key {
          mapped_ids.insert(id);
        }
      }
    }
    let inventory = state
      .inner
      .docker_api
      .list_containers(Some(ListContainersOptions {
        all: true,
        filters: Some(HashMap::from([(
          "label".to_owned(),
          vec![format!("io.nanocl.c={}", candidate.resource_key)],
        )])),
        ..Default::default()
      }))
      .await
      .map_err(|_| {
        deferred("Docker inventory unavailable; operation remains pending")
      })?;
    if candidate.action == "delete" {
      if !deletion_observation_settles(
        inventory.is_empty(),
        processes.is_empty(),
        mapped_ids.is_empty(),
      ) {
        return Err(deferred(
          "Cargo deletion is not confirmed by empty runtime inventory, process rows, and attachments",
        ));
      }
      // Runtime removal also removes Process rows before the definition is
      // deleted. Do not tombstone while stale rows still need reconciliation.
      return Ok(None);
    }
    if ids.iter().any(|id| !mapped_ids.contains(*id))
      || mapped_ids.iter().any(|id| !ids.contains(id.as_str()))
    {
      return Err(deferred(
        "Runtime process rows and current attachments do not fully agree",
      ));
    }
    let mut inventory_ids = HashSet::new();
    for container in inventory {
      let id = container
        .id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
          deferred("Docker inventory has no immutable identity")
        })?;
      if !ids.contains(id) {
        return Err(deferred(
          "An unassociated Cargo container makes execution uncertain",
        ));
      }
      if container.names.as_ref().is_some_and(|names| {
        names
          .iter()
          .any(|name| super::is_rollout_process_name(name))
      }) {
        return Err(deferred(
          "A retained or candidate runtime requires explicit reconciliation",
        ));
      }
      let process =
        super::validate_target(&cargo, candidate.incarnation_key, id, state)
          .await?;
      let persisted_labels = process
        .data
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref());
      if !observed_identity_matches(container.labels.as_ref(), persisted_labels)
      {
        return Err(deferred(
          "Docker runtime labels do not match the owned process identity",
        ));
      }
      if matches!(candidate.action.as_str(), "start" | "update")
        && !accepted_creation_identity(
          container.labels.as_ref(),
          persisted_labels,
          candidate.incarnation_key,
          candidate.spec_key,
        )
      {
        return Err(deferred(
          "Runtime creation identity does not match its owned accepted specification",
        ));
      }
      inventory_ids.insert(id.to_owned());
    }
    for id in ids {
      if inventory_ids.contains(id) {
        continue;
      }
      super::validate_target(&cargo, candidate.incarnation_key, id, state)
        .await?;
      // A runtime with changed Cargo labels disappears from the filtered
      // inventory. Only a confirmed absent immutable ID may be omitted.
      match state.inner.docker_api.inspect_container(id, None).await {
        Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 404,
          ..
        }) => {}
        _ => {
          return Err(deferred(
            "A mapped process is not confirmed absent from Docker inventory",
          ));
        }
      }
    }
    let observed = super::observe(&cargo, state).await?;
    if !runtime_observation_settles(
      &candidate.action,
      observed.actual.as_deref(),
    ) {
      return Err(deferred(
        "Runtime observation cannot safely settle the abandoned operation",
      ));
    }
    Ok(Some(observed))
  };
  let observed = tokio::time::timeout(Duration::from_secs(5), observation)
    .await
    .map_err(|_| {
      deferred("Runtime observation timed out; operation remains pending")
    })??;
  let finished = match observed {
    None => {
      CargoDb::delete_operation(candidate, lease.clone(), &state.inner.pool)
        .await?
    }
    Some(observed) => CargoDb::finish_operation(candidate, observed, "interrupted", Some(serde_json::json!({
      "Operation": candidate.key, "Phase": candidate.action,
      "Failure": "Execution ownership was lost; local runtime observed without replay"
    })), lease.clone(), &state.inner.pool).await?,
  };
  if finished {
    if let Err(error) =
      utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
    {
      log::warn!("Cargo recovery owner cleanup deferred: {error}");
    }
    if candidate.action == "delete" {
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Destroy)
        .await;
      return Ok(true);
    }
    let action = match candidate.action.as_str() {
      "start" => NativeEventAction::Starting,
      "stop" => NativeEventAction::Stopping,
      "update" => NativeEventAction::Updating,
      "delete" => NativeEventAction::Destroying,
      _ => NativeEventAction::Restart,
    };
    state.emit_error_native_action(&cargo, action, Some(format!("Cargo operation {} ({}) was interrupted; its local runtime was observed without replay", candidate.key, candidate.action)));
  }
  Ok(finished)
}

fn runtime_observation_settles(action: &str, actual: Option<&str>) -> bool {
  // These observations do not prove the requested action succeeded. Once the
  // caller has proved abandoned local ownership and associated runtime, they
  // allow recording that state with an interrupted outcome without replay.
  // Delete still requires its separate empty-runtime proof and transaction.
  matches!(
    action,
    "start"
      | "update"
      | "stop"
      | "restart"
      | "stop_process"
      | "restart_process"
      | "signal"
  ) && matches!(actual, Some("start" | "stop" | "fail"))
}

fn deletion_observation_settles(
  inventory_empty: bool,
  processes_empty: bool,
  attachments_empty: bool,
) -> bool {
  inventory_empty && processes_empty && attachments_empty
}

fn observed_identity_matches(
  labels: Option<&HashMap<String, String>>,
  persisted_labels: Option<&HashMap<String, String>>,
) -> bool {
  use crate::utils::container::cargo_compiler::{
    LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_KIND,
    LABEL_REPLICA, LABEL_REPLICA_ORDINAL, LABEL_ROLE,
  };
  let (Some(labels), Some(persisted)) = (labels, persisted_labels) else {
    return false;
  };
  // validate_target proves ownership of the persisted identity. Compare the
  // actual Docker identity exactly, including retained creation labels rather
  // than substituting the currently accepted operation or specification.
  [
    LABEL_KIND,
    LABEL_CARGO_KEY,
    LABEL_REPLICA,
    LABEL_REPLICA_ORDINAL,
    LABEL_CONTAINER,
    LABEL_ROLE,
    LABEL_ESSENTIAL,
  ]
  .iter()
  .all(|label| {
    persisted
      .get(*label)
      .is_some_and(|expected| labels.get(*label) == Some(expected))
  }) && [
    super::LABEL_INCARNATION,
    super::LABEL_SPEC,
    super::LABEL_OPERATION,
  ]
  .iter()
  .all(|label| labels.get(*label) == persisted.get(*label))
}

fn accepted_creation_identity(
  labels: Option<&HashMap<String, String>>,
  persisted_labels: Option<&HashMap<String, String>>,
  incarnation: uuid::Uuid,
  spec: uuid::Uuid,
) -> bool {
  let parsed = |labels: Option<&HashMap<String, String>>, label: &str| {
    labels
      .and_then(|labels| labels.get(label))
      .and_then(|value| value.parse::<uuid::Uuid>().ok())
  };
  let Some(operation) = parsed(persisted_labels, super::LABEL_OPERATION) else {
    return false;
  };
  // validate_target has checked this retained creation operation's resource,
  // incarnation and specification. Reused containers keep that operation ID;
  // it need not equal the operation being recovered.
  [
    (super::LABEL_INCARNATION, incarnation),
    (super::LABEL_SPEC, spec),
  ]
  .iter()
  .all(|(label, expected)| {
    parsed(labels, label) == Some(*expected)
      && parsed(persisted_labels, label) == Some(*expected)
  }) && parsed(labels, super::LABEL_OPERATION) == Some(operation)
}

pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  ntex::rt::spawn(async move {
    let mut after = None;
    loop {
      match OperationDb::pending_for_node(
        "Cargo",
        &state.inner.config.hostname,
        after,
        32,
        &state.inner.pool,
      )
      .await
      {
        Ok(candidates) => {
          after = if candidates.len() == 32 {
            candidates.last().map(|operation| operation.key)
          } else {
            None
          };
          for candidate in candidates {
            if let Err(error) = recover_one(&candidate, &state).await {
              log::warn!(
                "Cargo operation {} recovery deferred: {error}",
                candidate.key
              );
            }
          }
        }
        Err(error) => log::warn!("Cargo recovery scan deferred: {error}"),
      }
      ntex::time::sleep(Duration::from_secs(10)).await;
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_stubs::system::ObjPsStatusKind;

  #[test]
  fn abandoned_local_actions_accept_observed_state_without_success_inference() {
    for action in [
      "start",
      "update",
      "stop",
      "restart",
      "restart_process",
      "stop_process",
      "signal",
    ] {
      for state in [
        ObjPsStatusKind::Start,
        ObjPsStatusKind::Stop,
        ObjPsStatusKind::Fail,
      ] {
        assert!(runtime_observation_settles(
          action,
          Some(&state.to_string())
        ));
      }
      for actual in [
        None,
        Some("starting"),
        Some("stopping"),
        Some("updating"),
        Some("<unknown>"),
      ] {
        assert!(
          !runtime_observation_settles(action, actual),
          "{action} must remain pending without a known observed state"
        );
      }
    }
    for action in ["delete", "unknown"] {
      for actual in [None, Some("start"), Some("stop"), Some("fail")] {
        assert!(!runtime_observation_settles(action, actual));
      }
    }
  }

  #[test]
  fn delete_recovery_requires_empty_inventory_processes_and_mappings() {
    for inventory_empty in [true, false] {
      for processes_empty in [true, false] {
        for attachments_empty in [true, false] {
          let settled = deletion_observation_settles(
            inventory_empty,
            processes_empty,
            attachments_empty,
          );
          if !inventory_empty || !processes_empty || !attachments_empty {
            assert!(
              !settled,
              "Remaining runtime evidence must prevent deleting the definition"
            );
          } else {
            assert!(settled);
          }
        }
      }
    }
  }

  #[test]
  fn creation_recovery_accepts_reused_owned_identity_but_rejects_unproven_generations()
   {
    let incarnation = uuid::Uuid::new_v4();
    let spec = uuid::Uuid::new_v4();
    let operation = uuid::Uuid::new_v4();
    let labels = HashMap::from([
      (
        super::super::LABEL_INCARNATION.into(),
        incarnation.to_string(),
      ),
      (super::super::LABEL_SPEC.into(), spec.to_string()),
      (super::super::LABEL_OPERATION.into(), operation.to_string()),
    ]);
    assert!(accepted_creation_identity(
      Some(&labels),
      Some(&labels),
      incarnation,
      spec,
    ));
    assert!(!accepted_creation_identity(
      Some(&labels),
      Some(&labels),
      incarnation,
      uuid::Uuid::new_v4(),
    ));
    assert!(!accepted_creation_identity(
      Some(&labels),
      Some(&labels),
      uuid::Uuid::new_v4(),
      spec,
    ));
    assert!(!accepted_creation_identity(
      None,
      Some(&labels),
      incarnation,
      spec,
    ));
    for value in [
      None,
      Some("invalid".to_owned()),
      Some(uuid::Uuid::new_v4().to_string()),
    ] {
      let mut observed = labels.clone();
      match value {
        Some(value) => {
          observed.insert(super::super::LABEL_OPERATION.into(), value);
        }
        None => {
          observed.remove(super::super::LABEL_OPERATION);
        }
      }
      assert!(!accepted_creation_identity(
        Some(&observed),
        Some(&labels),
        incarnation,
        spec
      ));
    }
    assert!(!accepted_creation_identity(
      Some(&labels),
      None,
      incarnation,
      spec
    ));
  }

  #[test]
  fn recovery_requires_actual_docker_identity_to_match_owned_process() {
    use crate::utils::container::cargo_compiler::{
      LABEL_CARGO_KEY, LABEL_CONTAINER, LABEL_ESSENTIAL, LABEL_KIND,
      LABEL_REPLICA, LABEL_REPLICA_ORDINAL, LABEL_ROLE,
    };
    let labels = HashMap::from([
      (LABEL_KIND.into(), "cargo".into()),
      (LABEL_CARGO_KEY.into(), "global.api".into()),
      (LABEL_REPLICA.into(), uuid::Uuid::new_v4().to_string()),
      (LABEL_REPLICA_ORDINAL.into(), "0".into()),
      (LABEL_CONTAINER.into(), "api".into()),
      (LABEL_ROLE.into(), "app".into()),
      (LABEL_ESSENTIAL.into(), "true".into()),
      (
        super::super::LABEL_INCARNATION.into(),
        uuid::Uuid::new_v4().to_string(),
      ),
      (
        super::super::LABEL_SPEC.into(),
        uuid::Uuid::new_v4().to_string(),
      ),
      (
        super::super::LABEL_OPERATION.into(),
        uuid::Uuid::new_v4().to_string(),
      ),
    ]);
    assert!(observed_identity_matches(Some(&labels), Some(&labels)));
    assert!(!observed_identity_matches(None, Some(&labels)));
    assert!(!observed_identity_matches(Some(&labels), None));
    for label in labels.keys() {
      let mut changed = labels.clone();
      changed.insert(label.clone(), "different".into());
      assert!(
        !observed_identity_matches(Some(&changed), Some(&labels)),
        "{label}"
      );
      changed.remove(label);
      assert!(
        !observed_identity_matches(Some(&changed), Some(&labels)),
        "{label}"
      );
    }
    let mut missing_slot = labels.clone();
    missing_slot.remove(LABEL_REPLICA);
    assert!(!observed_identity_matches(
      Some(&missing_slot),
      Some(&missing_slot)
    ));
  }
}
