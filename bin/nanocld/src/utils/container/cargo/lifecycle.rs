use super::{
  LABEL_INCARNATION, LABEL_OPERATION, LABEL_SPEC, all_desired_replicas_ready,
  is_rollout_process_name, process_identity, stop_process,
};
use crate::{
  models::{
    CargoDb, CargoLifecycleLock, CargoLifecycleLocks, CargoReplicaDb,
    CargoReplicaProcessDb, CargoRuntimeContext, ObjPsStatusDb,
    ObjPsStatusUpdate, OperationDb, ProcessDb, SystemState,
  },
  repositories::generic::*,
};
use bollard_next::query_parameters::{
  KillContainerOptions, RemoveContainerOptions,
};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  cargo::Cargo,
  process::{Process, ProcessKind},
  system::{ObjPsHealthStatusKind, ObjPsStatusKind},
};
use std::{
  collections::HashMap,
  sync::{Arc, LazyLock, Mutex},
};

static LOCKS: LazyLock<CargoLifecycleLocks> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn lifecycle_lock(key: &str) -> CargoLifecycleLock {
  let mut locks = LOCKS.lock().unwrap_or_else(|error| error.into_inner());
  locks.retain(|_, lock| lock.strong_count() > 0);
  if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
    return lock;
  }
  let lock = Arc::new(tokio::sync::Mutex::new(()));
  locks.insert(key.to_owned(), Arc::downgrade(&lock));
  lock
}

pub(crate) async fn ensure_current(
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  if !CargoDb::validate_operation(
    &state.operation,
    state.lease.clone(),
    &state.inner.pool,
  )
  .await?
  {
    return Err(IoError::with_context(
      "Cargo operation",
      std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "Cargo operation no longer owns its accepted incarnation and specification",
      ),
    ));
  }
  Ok(())
}

pub(super) async fn set_health(
  key: &str,
  health: &ObjPsHealthStatusKind,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  ObjPsStatusDb::update_owned(
    key,
    ObjPsStatusUpdate {
      health: Some(health.to_string()),
      ..Default::default()
    },
    Some(state.operation.owner()),
    state.lease.clone(),
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

pub(super) async fn delete_instance(
  id: &str,
  options: Option<RemoveContainerOptions>,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  validate_process(id, state).await?;
  match state.inner.docker_api.remove_container(id, options).await {
    Ok(())
    | Err(bollard_next::errors::Error::DockerResponseServerError {
      status_code: 404,
      ..
    }) => {}
    Err(_) => {
      return Err(IoError::interrupted(
        "DeleteProcess",
        "Docker did not confirm removal of the Cargo process",
      ));
    }
  }
  ProcessDb::delete_owned(
    id,
    Some(state.operation.owner()),
    state.lease.clone(),
    &state.inner.pool,
  )
  .await
}

pub(super) async fn validate_process(
  id: &str,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  ensure_current(state).await?;
  ProcessDb::validate_cargo_owned(
    id,
    state.operation.owner(),
    state.lease.clone(),
    &state.inner.pool,
  )
  .await
}

/// Resolve once and prove the runtime's durable replica belongs to this Cargo.
/// Subsequent Docker mutations target the immutable ID, never its reusable name.
pub(crate) async fn validate_target(
  cargo: &Cargo,
  incarnation: uuid::Uuid,
  id: &str,
  state: &SystemState,
) -> IoResult<Process> {
  let process = ProcessDb::transform_read_by_pk(id, &state.inner.pool).await?;
  if process.kind != ProcessKind::Cargo
    || process.kind_key != cargo.spec.cargo_key
    || process.node_name != state.inner.config.hostname
  {
    return Err(IoError::invalid_input(
      "Cargo process",
      "Process is not a local runtime of this Cargo",
    ));
  }
  let identity = process_identity(&process)?;
  let replica =
    CargoReplicaDb::get(identity.replica_key, &state.inner.pool).await?;
  if replica.cargo_key != cargo.spec.cargo_key
    || replica.ordinal != identity.ordinal
    || replica.node_name.as_deref()
      != Some(state.inner.config.hostname.as_str())
  {
    return Err(IoError::invalid_input(
      "Cargo process",
      "Process replica assignment changed",
    ));
  }
  let labels = process
    .data
    .config
    .as_ref()
    .and_then(|config| config.labels.as_ref());
  if let Some(owner) = labels.and_then(|labels| labels.get(LABEL_INCARNATION)) {
    if owner != &incarnation.to_string() {
      return Err(IoError::invalid_input(
        "Cargo process",
        "Process belongs to another Cargo incarnation",
      ));
    }
    let spec = labels
      .and_then(|labels| labels.get(LABEL_SPEC))
      .and_then(|value| value.parse::<uuid::Uuid>().ok());
    let operation = labels
      .and_then(|labels| labels.get(LABEL_OPERATION))
      .and_then(|value| value.parse::<uuid::Uuid>().ok());
    let (Some(spec), Some(operation)) = (spec, operation) else {
      return Err(IoError::invalid_input(
        "Cargo process",
        "Process has incomplete operation identity",
      ));
    };
    let operation = OperationDb::get(operation, &state.inner.pool)
      .await?
      .ok_or_else(|| {
        IoError::invalid_input(
          "Cargo process",
          "Process operation is not retained",
        )
      })?;
    if operation.resource_kind != "Cargo"
      || operation.resource_key != cargo.spec.cargo_key
      || operation.incarnation_key != incarnation
      || operation.spec_key != spec
    {
      return Err(IoError::invalid_input(
        "Cargo process",
        "Process specification and operation identities disagree",
      ));
    }
  } else {
    if labels.is_some_and(|labels| {
      labels.contains_key(LABEL_OPERATION) || labels.contains_key(LABEL_SPEC)
    }) {
      return Err(IoError::invalid_input(
        "Cargo process",
        "Process has incomplete operation identity",
      ));
    }
    // The persisted replica UUID is scoped to this live Cargo incarnation.
    // A promoted mapping may no longer reference its retained bootstrap ID.
  }
  Ok(process)
}

pub(crate) async fn stop(
  cargo: &Cargo,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  ensure_current(state).await?;
  let processes =
    ProcessDb::read_by_kind_key(&cargo.spec.cargo_key, None, &state.inner.pool)
      .await?;
  for process in processes {
    let process = validate_target(
      cargo,
      state.operation.incarnation_key,
      &process.key,
      state,
    )
    .await?;
    stop_process(&process, state).await?;
  }
  let observed = observe(cargo, state).await?;
  if observed.actual.as_deref()
    != Some(ObjPsStatusKind::Stop.to_string().as_str())
  {
    return Err(IoError::interrupted(
      "Cargo stop",
      "Docker has not confirmed every runtime stopped",
    ));
  }
  ObjPsStatusDb::update_owned(
    &cargo.spec.cargo_key,
    observed,
    Some(state.operation.owner()),
    state.lease.clone(),
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

pub(crate) async fn process_action(
  cargo: &Cargo,
  id: &str,
  signal: Option<&str>,
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  ensure_current(state).await?;
  validate_target(cargo, state.operation.incarnation_key, id, state).await?;
  validate_process(id, state).await?;
  match state.operation.action.as_str() {
    "stop_process" => {
      match state.inner.docker_api.stop_container(id, None).await {
        Ok(_)
        | Err(bollard_next::errors::Error::DockerResponseServerError {
          status_code: 304,
          ..
        }) => {}
        Err(error) => {
          return Err(targeted_docker_error(error, "StopCargoProcess"));
        }
      }
    }
    "restart_process" => {
      state
        .inner
        .docker_api
        .restart_container(id, None)
        .await
        .map_err(|error| {
          targeted_docker_error(error, "Cargo process restart")
        })?;
    }
    "signal" => {
      state
        .inner
        .docker_api
        .kill_container(
          id,
          Some(KillContainerOptions {
            signal: signal.unwrap_or("SIGKILL").to_owned(),
          }),
        )
        .await
        .map_err(|error| {
          targeted_docker_error(error, "Cargo process signal")
        })?;
    }
    _ => {
      return Err(IoError::invalid_input(
        "Cargo process",
        "Unsupported targeted operation",
      ));
    }
  }
  // A single process action does not establish the state of other replicas.
  // Docker events can refresh aggregate observations after this owner finishes.
  Ok(())
}

fn targeted_docker_error(
  error: bollard_next::errors::Error,
  phase: &str,
) -> IoError {
  if matches!(
    error,
    bollard_next::errors::Error::DockerResponseServerError {
      status_code: 400 | 404 | 409,
      ..
    }
  ) {
    IoError::invalid_input(
      "Cargo process rejection",
      "Docker rejected the targeted process action",
    )
  } else {
    IoError::interrupted(
      phase,
      "Docker did not confirm the targeted process action",
    )
  }
}

pub(super) fn runtime_docker_error(
  error: bollard_next::errors::Error,
  phase: &str,
) -> IoError {
  if matches!(
    error,
    bollard_next::errors::Error::DockerResponseServerError {
      status_code: 400 | 404 | 409,
      ..
    }
  ) {
    IoError::invalid_input(
      "Cargo runtime rejection",
      "Docker rejected the Cargo runtime action",
    )
  } else {
    IoError::interrupted(
      phase,
      "Docker did not confirm the Cargo runtime action",
    )
  }
}

/// Observe exact mappings without replaying or cleaning any runtime work.
/// This node's Docker API cannot settle remote execution.
pub(crate) async fn observe(
  cargo: &Cargo,
  state: &SystemState,
) -> IoResult<ObjPsStatusUpdate> {
  let replicas =
    CargoReplicaDb::list_by_cargo(&cargo.spec.cargo_key, &state.inner.pool)
      .await?;
  if replicas.iter().any(|replica| {
    replica.node_name.as_deref().is_some_and(|node| {
      node != state.inner.config.hostname && node != "init-test.nanocl.io"
    })
  }) {
    return Err(IoError::interrupted(
      "Cargo remote observation",
      "Remote execution remains uncertain; local inventory cannot settle the operation",
    ));
  }
  let mut processes = Vec::new();
  let mut missing = false;
  for replica in &replicas {
    for mapping in
      CargoReplicaProcessDb::list_by_replica(replica.key, &state.inner.pool)
        .await?
    {
      let required =
        super::requires_steady_state_readiness(mapping.role, mapping.essential);
      let Some(id) = mapping.process_key else {
        missing |= required;
        continue;
      };
      let mut process =
        ProcessDb::transform_read_by_pk(&id, &state.inner.pool).await?;
      process.data =
        match state.inner.docker_api.inspect_container(&id, None).await {
          Ok(runtime) => runtime,
          Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          }) => {
            missing |= required;
            continue;
          }
          Err(_) => {
            return Err(IoError::interrupted(
              "Cargo observation",
              "Docker runtime observation unavailable",
            ));
          }
        };
      if process.data.id.as_deref() != Some(id.as_str())
        || process
          .data
          .state
          .as_ref()
          .and_then(|state| state.running)
          .is_none()
      {
        return Err(IoError::invalid_data(
          "Cargo observation",
          "Docker runtime identity or running state is unknown",
        ));
      }
      processes.push(process);
    }
  }
  let running = processes.iter().any(|process| {
    process.data.state.as_ref().and_then(|state| state.running) == Some(true)
  });
  let replica_keys = replicas.iter().map(|replica| replica.key).collect();
  let ready = !missing
    && replicas.len() == cargo.spec.replicas
    && !processes
      .iter()
      .any(|process| is_rollout_process_name(&process.name))
    && all_desired_replicas_ready(cargo, &replica_keys, &processes);
  let actual = if ready {
    ObjPsStatusKind::Start
  } else if !running {
    ObjPsStatusKind::Stop
  } else {
    ObjPsStatusKind::Fail
  };
  Ok(ObjPsStatusUpdate {
    actual: Some(actual.to_string()),
    health: Some(
      if ready {
        ObjPsHealthStatusKind::Healthy
      } else {
        ObjPsHealthStatusKind::Unknown
      }
      .to_string(),
    ),
    ..Default::default()
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn targeted_docker_rejections_preserve_certainty_without_daemon_payloads() {
    for code in [400, 404, 409] {
      let error = targeted_docker_error(
        bollard_next::errors::Error::DockerResponseServerError {
          status_code: code,
          message: "sensitive daemon payload".into(),
        },
        "Cargo process signal",
      );
      assert_eq!(error.context(), Some("Cargo process rejection"));
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
      assert!(!error.to_string().contains("sensitive"));
      let error = runtime_docker_error(
        bollard_next::errors::Error::DockerResponseServerError {
          status_code: code,
          message: "sensitive daemon payload".into(),
        },
        "StartCargoProcess",
      );
      assert_eq!(error.context(), Some("Cargo runtime rejection"));
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
      assert!(!error.to_string().contains("sensitive"));
    }
    let error = targeted_docker_error(
      bollard_next::errors::Error::DockerResponseServerError {
        status_code: 500,
        message: "sensitive daemon payload".into(),
      },
      "Cargo process signal",
    );
    assert_eq!(error.context(), Some("Cargo process signal"));
    assert!(!error.to_string().contains("sensitive"));
    let error = runtime_docker_error(
      bollard_next::errors::Error::DockerResponseServerError {
        status_code: 500,
        message: "sensitive daemon payload".into(),
      },
      "StartCargoProcess",
    );
    assert_eq!(error.context(), Some("StartCargoProcess"));
    assert!(!error.to_string().contains("sensitive"));
  }
}
