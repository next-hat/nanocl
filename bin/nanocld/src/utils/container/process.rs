use bollard_next::container::{
  Config, CreateContainerOptions, InspectContainerOptions,
  RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use futures::StreamExt;
use futures_util::stream::FuturesUnordered;
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  cargo::CargoKillOptions,
  process::{Process, ProcessKind, ProcessPartial},
  system::{NativeEventAction, ObjPsStatusKind},
};

use crate::{
  models::{ObjPsStatusDb, ProcessDb, SystemState},
  repositories::generic::*,
};

/// Create a process (container) based on the kind and the item
pub async fn create(
  kind: &ProcessKind,
  name: &str,
  kind_key: &str,
  item: &Config,
  state: &SystemState,
) -> IoResult<Process> {
  let mut config = item.clone();
  let mut labels = item.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  labels.insert("io.nanocl.kind".to_owned(), kind.to_string());
  config.labels = Some(labels);
  let res = state
    .inner
    .docker_api
    .create_container(
      Some(CreateContainerOptions {
        name,
        ..Default::default()
      }),
      config,
    )
    .await
    .map_err(|err| err.map_err_context(|| "CreateProcess"))?;
  let inspect = state
    .inner
    .docker_api
    .inspect_container(&res.id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "CreateProcess"))?;
  let created_at = inspect.created.clone().unwrap_or_default();
  let new_instance = ProcessPartial {
    key: res.id,
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
  let process =
    ProcessDb::create_from(&new_instance, &state.inner.pool).await?;
  Process::try_from(process)
    .map_err(|err| err.map_err_context(|| "CreateProcess"))
}

/// Delete a single instance (container) by his name
pub async fn delete_instance(
  pk: &str,
  opts: Option<RemoveContainerOptions>,
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
  ProcessDb::del_by_pk(pk, &state.inner.pool).await?;
  Ok(())
}

/// Delete a group of instances (containers) by their names
pub async fn delete_instances(
  instances: &[String],
  state: &SystemState,
) -> IoResult<()> {
  instances
    .iter()
    .map(|id| async {
      delete_instance(
        id,
        Some(RemoveContainerOptions {
          force: true,
          ..Default::default()
        }),
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

/// Kill instances (containers) by their kind key
/// Eg: kill a (job, cargo, vm)
pub async fn kill_by_kind_key(
  pk: &str,
  opts: &CargoKillOptions,
  state: &SystemState,
) -> IoResult<()> {
  let processes = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .kill_container(&process.key, Some(opts.clone().into()))
      .await
      .map_err(|err| err.map_err_context(|| "KillProcess"))?;
  }
  Ok(())
}

/// Restart the group of process for a kind key
/// Eg: (job, cargo, vm, etc.)
/// When finished, a event is emitted to the system
pub async fn restart_instances(
  pk: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  let processes = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
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
  let processes =
    ProcessDb::read_by_kind_key(kind_pk, &state.inner.pool).await?;
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
pub async fn start_instances(
  kind_key: &str,
  kind: &ProcessKind,
  state: &SystemState,
) -> IoResult<()> {
  let processes =
    ProcessDb::read_by_kind_key(kind_key, &state.inner.pool).await?;
  for process in processes {
    state
      .inner
      .docker_api
      .start_container(
        &process.data.id.unwrap_or_default(),
        None::<StartContainerOptions<String>>,
      )
      .await
      .map_err(|err| err.map_err_context(|| "StartProcess"))?;
  }
  ObjPsStatusDb::update_actual_status(
    kind_key,
    &ObjPsStatusKind::Start,
    &state.inner.pool,
  )
  .await?;
  super::generic::emit(kind_key, kind, NativeEventAction::Start, state).await?;
  Ok(())
}
