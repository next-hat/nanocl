use bollard_next::models::HostConfig;
use bollard_next::{
  models::ContainerCreateBody as Config,
  query_parameters::{
    CreateContainerOptions, InspectContainerOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
  },
};
use futures::StreamExt;
use futures_util::stream::FuturesUnordered;
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  process::{Process, ProcessKind, ProcessPartial},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::models::{ObjPsStatusDb, OperationOwner, ProcessDb, SystemState};

fn create_process_error(
  kind: &ProcessKind,
  error: bollard_next::errors::Error,
) -> IoError {
  if *kind == ProcessKind::Cargo
    && matches!(
      error,
      bollard_next::errors::Error::DockerResponseServerError {
        status_code: 400 | 404 | 409,
        ..
      }
    )
  {
    // Docker definitively rejected creation; no new runtime can appear later
    // from this request. Keep uncertain transport/server failures pending.
    IoError::invalid_input(
      "Cargo process rejection",
      "Docker rejected Cargo process creation",
    )
  } else {
    *error.map_err_context(|| "CreateProcess")
  }
}

/// Create a process (container) based on the kind and the item
#[cfg(test)]
pub async fn create(
  kind: &ProcessKind,
  name: &str,
  kind_key: &str,
  item: &Config,
  state: &SystemState,
) -> IoResult<Process> {
  create_owned(kind, name, kind_key, item, None, (), state).await
}

pub(crate) async fn create_owned<G: Clone + Send + 'static>(
  kind: &ProcessKind,
  name: &str,
  kind_key: &str,
  item: &Config,
  owner: Option<OperationOwner>,
  keepalive: G,
  state: &SystemState,
) -> IoResult<Process> {
  let mut config = item.clone();
  let host_config = config.host_config.take().unwrap_or_default();
  let network_mode =
    super::network::resolve_network_mode(host_config.network_mode.clone());
  config.host_config = Some(HostConfig {
    network_mode: Some(network_mode.clone()),
    ..host_config
  });
  // Keep this defensive check at the final local Docker boundary. Workload
  // paths also preflight their unique networks before parallel/destructive
  // operations, but every process creation must remain safe on its own.
  let _ = super::network::ensure_network_exists(&network_mode, state).await?;
  let mut labels = item.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  labels.insert("io.nanocl.kind".to_owned(), kind.to_string());
  config.labels = Some(labels);
  let res = state
    .inner
    .docker_api
    .create_container(
      Some(CreateContainerOptions {
        name: Some(name.to_owned()),
        ..Default::default()
      }),
      config,
    )
    .await
    .map_err(|error| create_process_error(kind, error))?;
  let process_key = res.id;
  let result = async {
    let inspect = state
      .inner
      .docker_api
      .inspect_container(&process_key, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "CreateProcess"))?;
    let created_at = inspect.created.clone().unwrap_or_default();
    let new_instance = ProcessPartial {
      key: process_key.clone(),
      name: name.to_owned(),
      kind: kind.clone(),
      data: serde_json::to_value(&inspect)
        .map_err(|err| err.map_err_context(|| "CreateProcess"))?,
      node_name: state.inner.config.hostname.clone(),
      kind_key: kind_key.to_owned(),
      created_at: Some(
        chrono::NaiveDateTime::parse_from_str(
          &created_at,
          "%Y-%m-%dT%H:%M:%S%.fZ",
        )
        .map_err(|err| {
          IoError::interrupted(
            "CreateProcess",
            &format!("Error while creating process {err}"),
          )
        })?,
      ),
    };
    let process = ProcessDb::create_owned(
      &new_instance,
      owner.clone(),
      keepalive.clone(),
      &state.inner.pool,
    )
    .await?;
    Process::try_from(process)
      .map_err(|err| err.map_err_context(|| "CreateProcess"))
  }
  .await;
  if result.is_err() {
    let removed = state
      .inner
      .docker_api
      .remove_container(
        &process_key,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
      )
      .await;
    if *kind == ProcessKind::Cargo
      && !matches!(
        removed,
        Ok(())
          | Err(bollard_next::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
          })
      )
    {
      return Err(IoError::interrupted(
        "Cargo rollback",
        "Docker did not confirm cleanup of the newly created Cargo process",
      ));
    }
    let deleted = ProcessDb::delete_owned(
      &process_key,
      owner,
      keepalive,
      &state.inner.pool,
    )
    .await;
    if *kind == ProcessKind::Cargo && deleted.is_err() {
      return Err(IoError::interrupted(
        "Cargo rollback",
        "Cargo process cleanup could not be committed under its accepted owner",
      ));
    }
  }
  result
}

pub(crate) async fn delete_instance_owned<G: Send + 'static>(
  pk: &str,
  opts: Option<RemoveContainerOptions>,
  owner: Option<OperationOwner>,
  keepalive: G,
  state: &SystemState,
) -> IoResult<()> {
  match state.inner.docker_api.remove_container(pk, opts).await {
    Ok(_) => {}
    Err(err) => match &err {
      bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: _,
      } => {
        log::error!("Error while deleting container {pk}: {err}");
        if *status_code != 404 {
          return Err(IoError::interrupted(
            "DeleteProcess",
            &format!("Error while deleting container {pk}: {err}"),
          ));
        }
      }
      _ => {
        log::error!("Error while deleting container {pk}: {err}");
        return Err(IoError::interrupted(
          "DeleteProcess",
          &format!("Error while deleting container {pk}: {err}"),
        ));
      }
    },
  };
  ProcessDb::delete_owned(pk, owner, keepalive, &state.inner.pool).await?;
  Ok(())
}

/// Delete a group of instances (containers) by their names
#[cfg(test)]
pub async fn delete_instances(
  instances: &[String],
  state: &SystemState,
) -> IoResult<()> {
  delete_instances_owned(instances, None, (), state).await
}

pub(crate) async fn delete_instances_owned<G: Clone + Send + 'static>(
  instances: &[String],
  owner: Option<OperationOwner>,
  keepalive: G,
  state: &SystemState,
) -> IoResult<()> {
  instances
    .iter()
    .map(|id| async {
      delete_instance_owned(
        id,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
        owner.clone(),
        keepalive.clone(),
        state,
      )
      .await
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<IoResult<()>>>()
    .await
    .into_iter()
    .collect::<IoResult<()>>()
}

/// Restart the group of process for a kind key
/// Eg: (job, cargo, vm, etc.)
/// When finished, a event is emitted to the system
pub async fn restart_instances(
  pk: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  if *kind == ProcessKind::Vm {
    return super::vm::restart_with_options(pk, &Default::default(), state)
      .await;
  }
  let processes =
    ProcessDb::read_by_kind_key(pk, None, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .restart_container(&process.key, None)
      .await
      .map_err(|err| err.map_err_context(|| "RestartProcess"))?;
  }
  super::generic::emit(pk, kind, NativeEventAction::Restart, state).await?;
  Ok(())
}

/// Stop the group of containers for a kind key
/// Eg: (job, cargo, vm)
/// When finished, a event is emitted to the system
pub async fn stop_instances(
  kind_pk: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  if *kind == ProcessKind::Vm {
    return super::vm::stop_with_options(kind_pk, &Default::default(), state)
      .await;
  }
  let processes =
    ProcessDb::read_by_kind_key(kind_pk, None, &state.inner.pool).await?;
  log::debug!("stop_process_by_kind_pk: {kind_pk}");
  for process in processes {
    state
      .inner
      .docker_api
      .stop_container(
        &process.data.id.unwrap_or_default(),
        None::<StopContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "StopProcess"))?;
  }
  ObjPsStatusDb::update_actual_status(
    kind_pk,
    &ObjPsStatusKind::Stop,
    &state.inner.pool,
  )
  .await?;
  super::generic::emit(kind_pk, kind, NativeEventAction::Stop, state).await?;
  Ok(())
}

/// Start the group of process for a kind key
/// Eg: (job, cargo, vm, etc.)
/// When finished, a event is emitted to the system
pub(crate) async fn start_instances_owned<G: Send + 'static>(
  kind_key: &str,
  kind: &ProcessKind,
  emit_event: bool,
  owner: Option<OperationOwner>,
  keepalive: G,
  state: &SystemState,
) -> IoResult<()> {
  let filter = GenericFilter::new().r#where(
    "data",
    GenericClause::Contains(serde_json::json!({
      "Config": {
        "Labels": {
          "io.nanocl.not-init-c": "true"
        }
      }
    })),
  );
  let filter = if *kind == ProcessKind::Vm {
    filter.r#where("kind", GenericClause::Eq(kind.to_string()))
  } else {
    filter
  };
  let processes =
    ProcessDb::read_by_kind_key(kind_key, Some(filter), &state.inner.pool)
      .await?;
  for process in processes {
    if process.name.starts_with("tmp-") {
      continue;
    }
    log::debug!("starting process {}", process.name);
    state
      .inner
      .docker_api
      .start_container(
        &process.data.id.unwrap_or_default(),
        None::<StartContainerOptions>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "StartProcess"))?;
  }
  if emit_event {
    ObjPsStatusDb::update_actual_status_owned(
      kind_key,
      &ObjPsStatusKind::Start,
      owner,
      keepalive,
      &state.inner.pool,
    )
    .await?;
    super::generic::emit(kind_key, kind, NativeEventAction::Start, state)
      .await?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cargo_create_rejection_is_definitive_without_changing_other_kinds() {
    let response =
      |status_code| bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: "Docker response".into(),
      };
    for status in [400, 404, 409] {
      let error = create_process_error(&ProcessKind::Cargo, response(status));
      assert_eq!(error.context(), Some("Cargo process rejection"));
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
      for kind in [ProcessKind::Vm, ProcessKind::Job] {
        assert_eq!(
          create_process_error(&kind, response(status)).context(),
          Some("CreateProcess"),
        );
      }
    }
    assert_eq!(
      create_process_error(&ProcessKind::Cargo, response(500)).context(),
      Some("CreateProcess"),
    );
    let transport = bollard_next::errors::Error::IOError {
      err: std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "unconfirmed",
      ),
    };
    assert_eq!(
      create_process_error(&ProcessKind::Cargo, transport).context(),
      Some("CreateProcess"),
    );
  }
}
