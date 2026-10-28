use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
  time::Duration,
};

use bollard_next::query_parameters::ListContainersOptions;
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::system::{NativeEventAction, ObjPsStatusKind};

use crate::{
  models::{CargoDb, CargoReplicaDb, OperationDb, ProcessDb, SystemState},
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
      if matches!(candidate.action.as_str(), "start" | "update")
        && !accepted_creation_identity(
          container.labels.as_ref(),
          candidate.incarnation_key,
          candidate.spec_key,
          candidate.key,
        )
      {
        return Err(deferred(
          "Ready runtime does not prove execution of the accepted creation operation",
        ));
      }
      super::validate_target(&cargo, candidate.incarnation_key, id, state)
        .await?;
    }
    let observed = super::observe(&cargo, state).await?;
    if !creation_observation_settles(
      &candidate.action,
      observed.actual.as_deref(),
    ) {
      return Err(deferred(
        "Interrupted creation has no complete ready runtime; operation remains pending",
      ));
    }
    Ok(observed)
  };
  let observed = tokio::time::timeout(Duration::from_secs(5), observation)
    .await
    .map_err(|_| {
      deferred("Runtime observation timed out; operation remains pending")
    })??;
  let finished = CargoDb::finish_operation(candidate, observed, "interrupted", Some(serde_json::json!({
    "Operation": candidate.key, "Phase": candidate.action,
    "Failure": "Execution ownership was lost; local runtime observed without replay"
  })), lease.clone(), &state.inner.pool).await?;
  if finished {
    if let Err(error) =
      utils::operation::cleanup_owner(&lease.1, &state.inner.pool).await
    {
      log::warn!("Cargo recovery owner cleanup deferred: {error}");
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

fn creation_observation_settles(action: &str, actual: Option<&str>) -> bool {
  !matches!(action, "start" | "update")
    || actual == Some(ObjPsStatusKind::Start.to_string().as_str())
}

fn accepted_creation_identity(
  labels: Option<&HashMap<String, String>>,
  incarnation: uuid::Uuid,
  spec: uuid::Uuid,
  operation: uuid::Uuid,
) -> bool {
  [
    (super::LABEL_INCARNATION, incarnation),
    (super::LABEL_SPEC, spec),
    (super::LABEL_OPERATION, operation),
  ]
  .iter()
  .all(|(label, expected)| {
    labels
      .and_then(|labels| labels.get(*label))
      .and_then(|value| value.parse::<uuid::Uuid>().ok())
      .as_ref()
      == Some(expected)
  })
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

  #[test]
  fn interrupted_creation_requires_complete_ready_runtime() {
    for action in ["start", "update"] {
      assert!(!creation_observation_settles(action, None));
      assert!(!creation_observation_settles(action, Some("Stop")));
      assert!(!creation_observation_settles(action, Some("Fail")));
      assert!(creation_observation_settles(
        action,
        Some(&ObjPsStatusKind::Start.to_string())
      ));
    }
    for action in ["stop", "delete", "restart", "signal"] {
      assert!(creation_observation_settles(action, Some("Stop")));
    }
  }

  #[test]
  fn creation_recovery_rejects_previous_and_unproven_generations() {
    let incarnation = uuid::Uuid::new_v4();
    let spec = uuid::Uuid::new_v4();
    let operation = uuid::Uuid::new_v4();
    let mut labels = HashMap::from([
      (
        super::super::LABEL_INCARNATION.into(),
        incarnation.to_string(),
      ),
      (super::super::LABEL_SPEC.into(), spec.to_string()),
      (super::super::LABEL_OPERATION.into(), operation.to_string()),
    ]);
    assert!(accepted_creation_identity(
      Some(&labels),
      incarnation,
      spec,
      operation
    ));
    assert!(!accepted_creation_identity(
      Some(&labels),
      incarnation,
      uuid::Uuid::new_v4(),
      operation
    ));
    assert!(!accepted_creation_identity(
      Some(&labels),
      incarnation,
      spec,
      uuid::Uuid::new_v4()
    ));
    assert!(!accepted_creation_identity(
      None,
      incarnation,
      spec,
      operation
    ));
    labels.remove(super::super::LABEL_OPERATION);
    assert!(!accepted_creation_identity(
      Some(&labels),
      incarnation,
      spec,
      operation
    ));
  }
}
